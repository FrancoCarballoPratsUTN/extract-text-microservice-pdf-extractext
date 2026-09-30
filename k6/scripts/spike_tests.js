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

const SPIKE_VUS = Number(__ENV.SPIKE_VUS || 25);

export const options = {
  stages: [
    { duration: __ENV.SPIKE_RAMP || '10s', target: SPIKE_VUS },
    { duration: __ENV.SPIKE_HOLD || '20s', target: SPIKE_VUS },
    { duration: __ENV.SPIKE_DOWN || '10s', target: 0 },
  ],
  // El CA de mkcert no esta en el trust store del contenedor k6, que ademas
  // negotiate TLS con SNI=`traefik` (Traefik le sirve su cert autogenerado).
  insecureSkipTLSVerify: true,
  summaryTrendStats: ['avg', 'min', 'med', 'p(90)', 'p(95)', 'p(99)', 'max'],
  thresholds: {
    extract_failed: ['rate<0.05'],
    extract_duration: ['p(95)<5000'],
  },
};

// Carga de PDFs en modo binario durante la inicializacion (init context de k6)
const pdfFiles = [
  open('./pdfs/2020-Scrum-Guide-Spanish-Latin-South-American.pdf', 'b'),
  open('./pdfs/Essential-Kanban-Condensed-Spanish.pdf', 'b'),
  open('./pdfs/Filosofia Lean.pdf', 'b'),
  open('./pdfs/scrum_manager_historias_usuario.pdf', 'b'),
];

export default function () {
  // Seleccion aleatoria de un PDF de la lista
  const randomPdf = pdfFiles[Math.floor(Math.random() * pdfFiles.length)];

  const params = {
    headers: {
      Host: VHOST,
      'Content-Type': 'application/pdf',
    },
    tags: { name: 'extract' },
  };

  const res = http.post(`${BASE_URL}/extract`, randomPdf, params);
  const body = res.json();

  extractDuration.add(res.timings.duration);
  extractFailed.add(res.status !== 200);

  check(res, {
    'status 200': (r) => r.status === 200,
    'page_count == pages.length': () => body?.page_count === body?.pages?.length,
  });
}
