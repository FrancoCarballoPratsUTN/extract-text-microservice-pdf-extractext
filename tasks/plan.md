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
- [ ] **T6** · Parseo `lopdf` en memoria + validación `%PDF-`
- [ ] **T7** · Motor de extracción paralela por páginas (rayon, N hilos auto-detectados)

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