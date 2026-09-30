import http from 'k6/http';
import { check } from 'k6';
import { Rate, Trend } from 'k6/metrics';

const extractDuration = new Trend('extract_duration', true);
const extractFailed = new Rate('extract_failed');

// Traefik enruta por vhost: dentro de la red `mired` el nombre
// `extract-service.universidad.localhost` no resuelve, asi que nos conectamos
// al container `traefik` y sobreescribimos el header Host con el vhost.
const BASE_URL = __ENV.BASE_URL || 'https://traefik';
const VHOST = __ENV.VHOST || 'extract-service.universidad.localhost';

export const options = {
  vus: 1,
  iterations: 1,
  // El CA de mkcert no esta en el trust store del contenedor k6, que ademas
  // negotiate TLS con SNI=`traefik` (Traefik le sirve su cert autogenerado).
  insecureSkipTLSVerify: true,
  summaryTrendStats: ['avg', 'min', 'med', 'p(90)', 'p(95)', 'p(99)', 'max'],
  thresholds: {
    checks: ['rate==1'],
    http_req_failed: ['rate==0'],
    extract_failed: ['rate==0'],
  },
};

// Carga de PDFs en modo binario durante la inicializacion (init context de k6).
// El mas chico de la mezcla: el smoke no mide performance.
const pdf = open('./pdfs/2020-Scrum-Guide-Spanish-Latin-South-American.pdf', 'b');

// Por encima de EXTRACT_BODY_LIMIT_BYTES (15 MiB) para disparar el 413
const oversizedBody = new ArrayBuffer(16 * 1024 * 1024);

function post(body, contentType) {
  const headers = { Host: VHOST };
  if (contentType) {
    headers['Content-Type'] = contentType;
  }
  return http.post(`${BASE_URL}/extract`, body, {
    headers,
    tags: { name: 'extract' },
    // Los 400/413/415 son respuestas esperadas del contrato, no fallos
    responseCallback: http.expectedStatuses(200, 400, 413, 415),
  });
}

export default function () {
  // PDF valido
  const ok = post(pdf, 'application/pdf');
  const okBody = ok.json();

  extractDuration.add(ok.timings.duration);
  extractFailed.add(ok.status !== 200);

  check(ok, {
    'valid -> 200': (r) => r.status === 200,
    'valid -> page_count == pages.length': () => okBody?.page_count === okBody?.pages?.length,
  });

  // Sin firma %PDF-
  const invalid = post('%%%not-a-pdf%%%', 'application/octet-stream');

  check(invalid, {
    'invalid -> 400': (r) => r.status === 400,
    'invalid -> problem title': (r) => r.json('title') === 'Invalid PDF Signature',
  });

  // Content-Type no soportado
  const unsupported = post('%%%not-a-pdf%%%', 'application/json');

  check(unsupported, {
    'unsupported content-type -> 415': (r) => r.status === 415,
    'unsupported content-type -> problem title': (r) => r.json('title') === 'Unsupported Media Type',
  });

  // Body por encima del limite global
  const tooLarge = post(oversizedBody, 'application/pdf');

  check(tooLarge, {
    'too large -> 413': (r) => r.status === 413,
    'too large -> problem title': (r) => r.json('title') === 'Payload Too Large',
  });
}
