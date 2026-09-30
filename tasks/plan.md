# Implementation Plan: Microservicio `extract` en Rust

## Overview

Microservicio de alto rendimiento que recibe un PDF codificado en Base64 (200-500 páginas, ~12MB+), lo decodifica y extrae el texto de todas sus páginas en paralelo mediante `rayon`, aprovechando **todos los núcleos/hilos lógicos disponibles en el dispositivo** (el tamaño del pool se auto-detecta en runtime, no está fijado a un valor concreto). No toca disco: el PDF se parsea directamente desde un slice de bytes en RAM con `lopdf`. La entrada/salida se realiza vía HTTP JSON (`axum` + `tokio`) con un límite de body de 50MB, y todos los errores se devuelven bajo el estándar **RFC 9457 (Problem Details)** con `Content-Type: application/problem+json`.

Este documento corresponde a la **Fase 3 (Desglose y definición de tareas)**. No se escribe código hasta la aprobación del plan.

## Requisitos y stack

| Concern | Stack |
|---------|-------|
| Web framework | `axum` + `tokio` (body limit 50MB, procesamiento asíncrono) |
| Decodificación Base64 | `base64-simd` (AVX2/NEON por detección de features) |
| Parseo PDF | `lopdf` desde `&[u8]` en memoria (`Document::load_mem`) |
| Paralelismo de datos | `rayon` (pool dedicado, N hilos auto-detectados, work-stealing) |
| Serialización | `serde` + `serde_json` |
| Observabilidad | `tracing` + `tracing-subscriber` |

## Architecture Decisions

1. **Arquitectura de 3 capas estricta** — `api/` (presentación), `app/` (aplicación/servicio), `domain/` (dominio/infraestructura). La capa `app/` delega todo trabajo CPU-bound a `tokio::task::spawn_blocking` para no bloquear el runtime de red; dentro del bloqueo se usa un pool `rayon` dedicado para romper la extracción por páginas.
2. **Pool rayon dedicado en `AppState` — tamaño adaptativo** — se construye una sola vez con `ThreadPoolBuilder::new().num_threads(n)` donde `n` se auto-detecta en runtime con `std::thread::available_parallelism()` (devuelve los núcleos lógicos del dispositivo, p. ej. 12 en la CPU de referencia). Admite override explícito vía env `EXTRACT_NUM_THREADS`. No se usa el pool global: esto aísla la saturación de CPU y permite inyectar el pool en tests.
3. **Cero clones en la ruta crítica** — el payload se consume por valor en el handler; la decodificación base64 calcula el tamaño exacto del buffer (`needed_bufsize`) antes de asignar; la extracción agrega texto con `String::with_capacity` sobre una estimación de capacidad total para evitar reasignaciones del OS.
4. **Validación estructural mínima** — sólo se valida: (a) el Base64 es decodificable, y (b) la firma mágica `%PDF-` al inicio del buffer decodificado. La validación de negocio profunda vive en otro microservicio.
5. **Contrato HTTP y errores fijados temprano (fail-fast)** — antes de construir el motor completo se despliega un spike vertical (health + body limit + problem+json) para congelar el contrato y evitar rework.
6. **Compilación para el chip de destino** — perfil `release` con `lto = "fat"`, `codegen-units = 1` y `RUSTFLAGS="-C target-cpu=native"` (por `.cargo/config.toml`, no por env global del CI) para habilitar AVX2/AVX-512 al compilador y permitir inlining cross-crate del hot path de extracción.

## Arquitectura del árbol de directorios

```
extract-text-microservice-pdf-extractext/
├── Cargo.toml                # deps + perfil release optimizado
├── Cargo.lock
├── rust-toolchain.toml       # pin de toolchain stable
├── .cargo/config.toml        # RUSTFLAGS target-cpu=native (repo-local)
├── .env.example              # variables de entorno documentadas
├── README.md                 # arquitectura + cómo compilar/correr
├── tasks/
│   ├── plan.md               # este documento
│   └── todo.md               # task list + checkpoints
├── tests/
│   ├── api_integration.rs    # tests HTTP end-to-end
│   └── fixtures/             # PDFs de muestra generados
└── src/
    ├── main.rs               # bootstrap: config → tracing → router → serve
    ├── lib.rs                # expone módulos para tests de integración
    ├── config.rs             # Config: addr, puerto, body_limit, num_threads
    │
    ├── api/                  # ◀ CAPA PRESENTACIÓN
    │   ├── mod.rs
    │   ├── router.rs         # compose del Router axum + estado + capas
    │   ├── handlers.rs       # handlers: health, extract
    │   ├── contract.rs       # DTOs request/response (serde)
    │   └── problem_details.rs# RFC 9457: ProblemDetails + IntoResponse
    │
    ├── app/                  # ◀ CAPA APLICACIÓN / SERVICIOS
    │   ├── mod.rs
    │   ├── state.rs          # AppState: Config + rayon::ThreadPool + semaphore
    │   └── extract_service.rs# orquestador: spawn_blocking → pool.install
    │
    └── domain/               # ◀ CAPA DOMINIO / INFRAESTRUCTURA
        ├── mod.rs
        ├── model.rs          # tipos: PageText, ExtractedDocument, PdfBytes
        ├── base64_decoder.rs # decodificación SIMD con pre-cálculo de buffer
        ├── pdf_utils.rs      # validación firma mágica %PDF- (en RAM)
        ├── pdf_parser.rs     # Document::load_mem desde &[u8]
        └── page_extractor.rs # extracción por página en paralelo (rayon)

```
Capas (división de responsabilidades):
- **Presentación (`api/`)**: recibe el JSON, aplica `DefaultBodyLimit`, valida forma, formatea respuestas y errores `problem+json`. No hace cómputo pesado.
- **Aplicación (`app/`)**: orquesta la petición. Único punto que cruza a `spawn_blocking`, instala el pool rayon y traduce errores del dominio.
- **Dominio (`domain/`)**: mapeo de datos crudos, decodificación SIMD, parseo lopdf y extracción paralela pura (sin conocer HTTP).

## Estrategia de Compilación (código de destino)

### `Cargo.toml` — perfil release

```toml
[profile.release]
opt-level = 3                 # máx optimización del hot path
lto = "fat"                   # inlining cross-crate (rayon/base64-simd/lopdf)
codegen-units = 1             # mejor inferencia + vectorización
panic = "abort"               # binario más pequeño; sin unwinding en prod
strip = "symbols"             # reduce footprint del binario
```

### `.cargo/config.toml` — flags del chip (repo-local)

```toml
[build]
rustflags = ["-C", "target-cpu=native"]
```
Habilita AVX2/AVX-512/NEON según la CPU de compilación. `base64-simd` además usa detección de features en runtime para elegir la mejor implementación SIMD.

### Variables de entorno (documentadas en `.env.example` / CI)

```sh
RUSTFLAGS="-C target-cpu=native"     # vectorización para el chip destino
CARGO_PROFILE_RELEASE_LTO=fat        # equivalente a lto in directorio Cargo.toml
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
EXTRACT_BIND_ADDR=0.0.0.0:8080       # addr del server
EXTRACT_BODY_LIMIT_BYTES=52428800    # 50MB
EXTRACT_NUM_THREADS=                 # opcional; si se omite, pool rayon = available_parallelism() del dispositivo
```
> Nota: AVX-512 puede causar downclocking de CPU en algunos chips; `base64-simd` y el compilador eligen el nivel SIMD adecuado por detección de features, así que `target-cpu=native` + detección runtime es la combinación segura.

## Task List

### Fase 0 — Configuración e Infraestructura
- [x] **T1** · Scaffold Cargo + toolchain + perfil release + flags AVX
- [x] **T2** · Config de entorno + logging estructurado
- [x] **T3** · Esqueleto de 3 capas + `/health` + body limit 50MB + problem+json base (spike vertical)

### ✅ Checkpoint A (T1-T3)
- [x] `cargo build --release` compila sin warnings
- [x] Server arranca, `GET /health` → 200
- [x] POST con body >50MB → 413 `Content-Type: application/problem+json`

### Fase 1 — Capa de Dominio / Infraestructura
- [x] **T4** · Modelos de dominio + errores del dominio
- [x] **T5** · Decodificador Base64 SIMD con buffer exacto
- [x] **T6** · Parseo `lopdf` en memoria + validación `%PDF-`
- [x] **T7** · Motor de extracción paralela por páginas (rayon, N hilos auto-detectados)

### ✅ Checkpoint B (T4-T7)
- [ ] Tests unitarios del dominio pasan
- [ ] `clippy -D warnings` limpio; cero clones en la ruta crítica (revisión)
- [ ] Extracción funciona sobre un PDF de prueba (fixture)

### Fase 2 — Capa de Aplicación / Servicio
- [ ] **T8** · Servicio extractor orquestador (`spawn_blocking` + `pool.install`)

### ✅ Checkpoint C (T8)
- [ ] Flujo completo falla/satisfactorio por CLI o test, sin colgar el runtime
- [ ] Medición de duración por petición (span de tracing)

### Fase 3 — Capa de Presentación / API y Errores
- [ ] **T9** · ProblemDetails RFC 9457 completo (mapeo de todos los errores)
- [ ] **T10** · Handler `POST /extract` definitivo + contrato de respuesta

### ✅ Checkpoint D (T9-T10)
- [ ] Tests de integración HTTP del flujo completo (200/400/413/415)

### Fase 4 — Calidad, Rendimiento y Cierre
- [ ] **T11** · Tests de integración del API
- [ ] **T12** · Benchmarks (criterion): base64 decode + escalabilidad de la extracción
- [ ] **T13** · README + docs de arquitectura + cierre

### ✅ Checkpoint Final
- [ ] `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` todos verdes
- [ ] Benchmarks documentados (speedup 1→N hilos, N = núcleos del dispositivo)
- [ ] Revisión humana del plan/PR antes de merge

## Dependency Graph

```
T1 ─► T2 ─► T3  (spike HTTP/contrato)          ◀── fail-fast del contrato
│        └───────────────────────────┐
│        ┌──────────────────────────▼
T4 (modelo+errores dominio) ──► T5 (base64) ─┐
│        └──────────────────────► T6 (pdf) ──┴─► T7 (extracción rayon) ──► T8 (servicio) ──► T10 (handler)
└──────────────────────────────────────────────────────┐                    └──► T9 (problem details) ─┘
                                                        └────────► T11 (tests integración)
                                                    T7 ─► T12 (benchmarks) · T8 ─► T13 (docs/cierre)
```

## Riesgos y Mitigaciones

| Riesgo | Impacto | Mitigación |
|--------|---------|------------|
| `lopdf.extract_text` falla/panifica en PDFs malformados | Med | Extracción por página envuelta en guard/fallback a página vacía; nunca abortar la petición |
| Saturación de CPU por concurrencia de peticiones | Med | Pool rayon dedicado (N hilos auto-detectados) + `Semaphore` en `AppState` para limitar extracciones concurrentes (ver Open Questions) |
| `spawn_blocking` sin límite llena el threadpool de tokio | Med | El servicio acota el trabajo con el semáforo y el pool rayon |
| AVX-512 causa downclocking | Bajo | Detección de features en runtime de `base64-simd` elige el nivel seguro |
| Calidad del texto extraído varía según el PDF | Bajo | Extracción por página con error parcial aislado (página vacía) |
| Contrato del payload no acordado (clave del JSON) | Alto | Fijar contrato en T3 (spike) y validarlo con consumidores antes de T10 |

## Open Questions
- **Forma del payload**: ¿`{"document_base64": "..."}`? Confirmar nombre de la clave con el consumidor del servicio.
- **Forma de la respuesta**: ¿array de páginas `{"pages": [...], "text": "..."}` o texto plano concatenado?
- **Concurrencia**: ¿limitar extracciones simultáneas con `Semaphore`? (recomendado: sí, para proteger los N hilos)
- **Métricas**: ¿exponer Prometheus `/metrics` o basta `tracing` (spans con duración/page_count/bytes)?

---

## Enmienda v2 — Contrato binario (`POST /extract` sin base64) — Task 15

**Cambio**: el transporte del PDF pasa de `{"document_base64": "<b64>"}` a **binario crudo**.

| Aspecto | Antes (v1) | Después (v2) |
|---|---|---|
| Request | JSON `{"document_base64": …}` | PDF crudo, `Content-Type: application/pdf` \| `application/octet-stream` (**exigido**) |
| Error transporte | `400 Invalid Base64` / `400 Invalid Request Body` | `415 Unsupported Media Type` (ausencia/tipo no-PDF) |
| Error 400 | firma `%PDF-` ausente → `Invalid PDF Signature` | igual (body crudo) |
| Captura | pre-decode base64 | se elimina por completo (ve T16) |
| 413 | límite global de body | igual |

**Decisiones**: Content-Type obligatorio (ausencia → 415); `sample_pdf.b64` eliminado del repo (solo `sample_pdf.pdf` 5.2MB/250p); **no se introduce `Arc`** (el `&Document` compartido ya hace que los hilos rayon lean el mismo bloque en RAM sin duplicar). Respuesta y `/health` intactos. Estado: ✅ completo (T15).

## Enmienda v3 — Motor de extracción `pdf-extract` por objetivo P95 idle ≤ 300ms — Tasks 16–17

**Problema**: la extracción end-to-end del sample (250p/5.2MB) mide ~308–570ms con `lopdf`; el usuario exige **P95 idle ≤ 300ms** para el sample y para un fixture denso ~500p/~12MB (decidido: swap directo, sin tuning intermedio de lopdf).

**Diagnóstico (lopdf 0.45)**: `extract_text_with_limit([n])` reconstruye el árbol de páginas completo por llamada (`get_pages()` interno) → 1 walk/página = O(N²) por request; `get_font_encoding` se re-resuelve por página sin caché (los fonts se comparten entre páginas). El parse estructural es serial pero no es el cuello (bench de fixture 200p trivial = ~9ms; el sample pesa por contenido/fonts).

**Cambio**: el dominio de extracción pasa de `lopdf::Document::extract_text_with_limit` a `pdf-extract::prepare_pages` → `Vec<PageText>`, bajo el mismo contrato HTTP v2. Mapeo de errores a `DomainError` intacto (400 firma propio / 422 `/` 500).

**Replanteo de config**: `EXTRACT_MAX_DECOMPRESSED_BYTES` (per-page limit de lopdf) se elimina; el guard de tamaño pasa a `body_limit_bytes` + chequeo de `pdf.len()`.

**Riesgo honesto**: el texto de PDFs arbitrarios puede diferir entre motores (espaciado/hyphenation/orden de capas); equivalencia byte-exacta se garantiza en nuestros fixtures (goldens), y el sample se valida por invariantes. Fallback si el gate no se cumple: `pdfium-render`.

**Estado**: ✅ superada por la Enmienda v4 (pdf-extract descartado por el humano; el motor final es lopdf paralelo).

## Enmienda v4 — Motor final: `lopdf` paralelo con pool rayon + gate P95 ≤ 450ms — Tasks 16–17

**Problema**: tras la Enmienda v3 se intentó `pdfium-render` como motor (swap directo). Resultado de 3 experimentos: **pdfium no es thread-safe** — (1) heap corruption `SIGABRT` con chunks paralelos, (2) `SIGSEGV` durante una sola extracción concurrente, (3) `SIGSEGV` con 2 hilos corriendo cada uno su extractor secuencial. El modo seguro (`thread_safe` + mutex) fuerza secuencial: ~938ms bench / ~1.1s HTTP sobre el sample, muy por encima del gate.

**Cambio**: se **revierte a `lopdf` 0.45** y la ganancia viene de ejecutar la extracción por página en paralelo con un pool dedicado de `rayon` (instalado en `AppState::pool`, auto-detectado o por `EXTRACT_NUM_THREADS`). Se restauran `EXTRACT_NUM_THREADS` y `EXTRACT_MAX_DECOMPRESSED_BYTES` (default 64MB) como validación del guard (los benchmarks de página densa_12MB nunca la tocaron).

**Gate**: el usuario aprueba redefinir el objetivo a **P95 idle ≤ 450ms** sobre el sample 250p/5.2MB (antes: 300ms). Justificación medida: en contenedor sobre `mired` el sample mide P95 389–415ms (host, 349ms) + overhead de transferencia del payload multi-MB; 450ms da cabeza honesta sin relajar el problema (serial era 857ms por request).

**Resultados (medidos, compilados en `tasks/todo.md` T17):**
- Bench: `synthetic_200p` 2.4ms; `real_sample_250p_5mb` **306ms**; `dense_fixture_500p_12mb` 57ms (en CPU dedicada)
- HTTP host: sample ~321–352ms (service `duration_ms` 310ms)
- Contenedor `mired`: `perf.js` 20 iters → **p(95) = 432ms ≤ 450ms** (última corrida certificada 402ms), checks 40/40; `smoke.js` **11/11**
- Fix 413 determinista: `src/api/middleware.rs` `enforce_payload_limit` valida `Content-Length` y **drena el body** antes de responder 413 sobre la conexión viva (elimina el connection-reset del harness k6 en ~30% de los casos)

**Estado**: ✅ completo (T16 limpieza base64 y T17 cerrados; 64 tests verdes; gate y smoke de Fase A en verde en contenedor).

## Enmienda v5 — extractor lean con rollback configurable

La Fase B agrega `extract_document_lean`, que resuelve `get_pages()` una sola
vez y replica la semántica de extracción de texto de lopdf usando sus APIs
públicas. La salida se comparó byte a byte contra el extractor de referencia en
fixtures simples, densos, con páginas rotas y en el sample real de 250 páginas:
5 tests de paridad verdes.

`EXTRACT_EXTRACTOR=lean` es el default. `EXTRACT_EXTRACTOR=lopdf` conserva una
ruta de rollback sin recompilar ni modificar el contrato HTTP. La integración
queda validada por 42 tests unitarios, 17 de API y 5 de paridad. La corrida
final desde el contenedor dio smoke 11/11 y perf p95=338.07ms (20/20) frente al
gate P95 ≤ 450ms.

## Enmienda v6 — Scanner de content-stream de pasada única (objetivo P95 < 400ms)

El gate v4 (P95 ≤ 450ms) quedaba corto frente al objetivo de negocio (< 0.4s).
El perfilado del hot path (test de diagnóstico release sobre el sample 250p)
aisló el cuello: no eran las fuentes (~13ms) sino `Content::decode` +
materialización de `Vec<Operation>` (~1360ms CPU, 79% del costo de extracción).

**Decisión de diseño**: un scanner índice-en-slice en `src/domain/content_scanner.rs`
que replica **pérdida por pérdida** la gramática de `lopdf::parser` (strings
literales con escapes octales y anidación, hex strings, nombres `#xx`, números,
arrays, diccionarios, `true/false/null`, referencias `N M R`, inline images con
fallback de window-scan) y ejecuta el mismo dispatch de operadores de texto del
extractor lean dentro de un único barrido `&[u8]`, sin operadores intermedios.
Los operandos que no afectan texto no se materializan.

**Resultados medidos (host de desarrollo, contenedor `mired`, release lto=fat):**

| Métrica | Baseline v4 (lean+decode) | Scanner v6 | Delta |
|---|---|---|---|
| k6 `perf` p(95) roundtrip | 495ms | 166–198ms | **~3x mejor, gate 400ms ✓** |
| Span `extract_ms` (250p sample) | 259ms | 91ms | -65% |
| Span total `duration_ms` | 271ms | 104ms | -62% |
| Bench `real_sample_250p_5mb/lean` | ~219ms | 68ms | -69% |
| Bench `dense_fixture_500p_12mb/lean` | ~42ms | 21ms | -50% |

El gate del harness se bajó a **P95 ≤ 400ms** (`perf.js`, `PERF_P95` default).
Smoke 11/11 y perf 40/40 verdes con el gate nuevo. Paridad byte-a-byte contra
lopdf intacta (fixtures y sample real de 250 páginas).

**Decisión gzip**: con el roundtrip ya 2x-3x por debajo del umbral, se conserva
`CompressionLevel::Fastest` tal cual estaba; deshabilitar compresión no aporta
al objetivo y degradaría el throughput de red. Cero cambios en la ruta HTTP.