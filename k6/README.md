# k6 — Smoke y spike test contra `extract` vía Traefik

| Script | Qué hace | Run |
|---|---|---|
| `scripts/smoke.js` | 1 VU / 1 iteración. Valida el contrato HTTP completo: `200`, `400`, `415`, `413` | `docker compose run --rm k6 run /scripts/smoke.js` |
| `scripts/spike_tests.js` | Pico de N VUs concurrentes. Mide si el servicio aguanta la carga | `docker compose run --rm k6 run /scripts/spike_tests.js`

## Endpoint

El test pega **a través de Traefik**, no directo al contenedor. Traefik enruta
por vhost, y dentro de la red `mired` el nombre
`extract-service.universidad.localhost` no resuelve, así que el harness se
conecta al container `traefik` y sobreescribe el header `Host`:

| | |
|---|---|
| URL | `https://traefik/extract` |
| Header | `Host: extract-service.universidad.localhost` |
| Body | PDF crudo (`Content-Type: application/pdf`) |

Es equivalente a, desde el host:

```bash
curl -X POST --data-binary @scripts/pdfs/Filosofia\ Lean.pdf \
  -H 'Content-Type: application/pdf' \
  https://extract-service.universidad.localhost/extract
```

`insecureSkipTLSVerify` está activo porque el CA de mkcert no está en el trust
store del contenedor k6 (que además negocia SNI=`traefik`, a lo que Traefik le
sirve su certificado autogenerado).

## Configuración por variables de entorno

| Variable | Default | Qué es |
|---|---|---|
| `SPIKE_VUS` | `25` | VUs del pico |
| `SPIKE_RAMP` | `10s` | dur. de la subida |
| `SPIKE_HOLD` | `20s` | dur. del sostén |
| `SPIKE_DOWN` | `10s` | dur. de la bajada |
| `BASE_URL` | `https://traefik` | upstream |
| `VHOST` | `extract-service.universidad.localhost` | vhost de Traefik |

```bash
SPIKE_VUS=100 docker compose run --rm k6 run /scripts/spike_tests.js
```

## Umbrales

### smoke

| Métrica | Umbral |
|---|---|
| `checks` | `rate==1` |
| `http_req_failed` | `rate==0` |
| `extract_failed` | `rate==0` |

Los 8 checks cubren el contrato: PDF válido (`200` + `page_count === pages.length`),
firma `%PDF-` ausente (`400`), `Content-Type` no soportado (`415`) y body sobre
`EXTRACT_BODY_LIMIT_BYTES` (`413`), cada uno validando además el `title` del
problem+json. Los 400/413/415 son respuestas esperadas, por eso el request usa
`responseCallback: http.expectedStatuses(200, 400, 413, 415)` — sin eso k6 los
cuenta como `http_req_failed` y la métrica miente.

`GET /health` no se testea acá: el router de Traefik matchea
`PathPrefix('/extract')`, así que `/health` da 404 por proxy. El health check va
directo al contenedor: `docker exec extract-service curl -s http://127.0.0.1:8000/health`.

### spike_tests

| Métrica | Umbral | Default |
|---|---|---|
| `extract_failed` | `rate<0.05` | |
| `extract_duration` | `p(95)<5000` | |

Además `checks`: `status 200` y `page_count === pages.length`.

## Resultados medidos (host de desarrollo, 6 cores / 14 GB, compartido)

| Test | Resultado |
|---|---|
| `smoke` | ✓ verde — 8/8 checks, 4 requests, ~77 ms/iteración |
| `spike` 25 VUs (default) | ✓ verde — 1132 req, 0 errores, p95 **1.72 s**, `extract-service` ~1 GiB |
| `spike` 100 VUs | 0 errores, pero **p95 6.12 s** → cruza el umbral (exit 99) |

A 100 VUs la latencia se acumula en cola: el host tiene 6 cores y el pico manda
~94 MB/s de PDFs al servicio, que parsea en RAM. El pico de memoria del
microservicio llegó a 2.4 GiB sin OOM (el OOM que documentaba la versión
anterior del harness no se reprodujo). Para umbrales de 100 VUs hace falta un
host con CPU/RAM dedicadas o subir `p(95)` en el script.

## Payloads

`scripts/pdfs/` se monta en `/scripts` y se carga en el init context de k6
(`open(..., 'b')`), una vez por instancia, no por request. La mezcla es
aleatoria uniforme entre los 4 PDFs (0.3 MB – 8.9 MB). Todos están por debajo
del límite de body de 15 MiB (`EXTRACT_BODY_LIMIT_BYTES`), así que ninguno
dispara el `413`.
