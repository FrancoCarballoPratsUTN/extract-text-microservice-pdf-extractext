# Task List — Microservicio `extract`

> Plan completo en `tasks/plan.md`. Tareas ordenadas por dependencia; agrupadas por Fase. Estado de cada tarea: `[ ]` pendiente, `[x]` completada.

---

## Fase 0 — Configuración e Infraestructura

### Task 1: Scaffold del proyecto Cargo + perfil release + flags AVX

**Description:** Crea el proyecto Cargo con la toolchain estable (`rust-toolchain.toml`), todas las dependencias del stack (axum, tokio, base64-simd, lopdf, rayon, serde, serde_json, tracing, tracing-subscriber) y el perfil `release` optimizado. Se configura `.cargo/config.toml` con `RUSTFLAGS="-C target-cpu=native"`.

**Acceptance criteria:**
- [x] `Cargo.toml` declara todo el stack + perfil `[profile.release]` con `opt-level=3`, `lto="fat"`, `codegen-units=1`, `panic="abort"`, `strip="symbols"`
- [x] `.cargo/config.toml` aplica `target-cpu=native`; `rust-toolchain.toml` fija stable
- [x] `cargo build --release` compila el binario mínimo sin warnings

**Verification:**
- [x] Tests pass: `cargo build --release` (primera compilación exitosa, logs del compilador sin warnings)
- [x] Manual check: `./target/release/<bin> --version` ejecuta correctamente → `extract 0.1.0`

**Dependencies:** None

**Files likely touched:**
- `Cargo.toml`
- `rust-toolchain.toml`
- `.cargo/config.toml`
- `src/main.rs`

**Estimated scope:** Small (2-4 archivos de config + main mínimo)

---

### Task 2: Configuración de entorno + logging estructurado

**Description:** Módulo `config.rs` que lee configuración por env vars (`EXTRACT_BIND_ADDR`, `EXTRACT_BODY_LIMIT_BYTES`, `EXTRACT_NUM_THREADS`) con defaults sensatos, y se inicializa `tracing_subscriber` para logs estructurados en consola. El tamaño del pool rayon se auto-detecta del dispositivo (sin valor fijo) y `EXTRACT_NUM_THREADS` sólo lo sobreescribe de forma opcional. Se documentan las vars en `.env.example`.

**Acceptance criteria:**
- [x] `Config::from_env()` carga addr/puerto, body limit (default 50MB) y num_threads (default: `std::thread::available_parallelism()` del dispositivo, override opcional con `EXTRACT_NUM_THREADS`)
- [x] `main.rs` inicializa tracing y arranca con la config
- [x] `.env.example` documenta todas las variables

**Verification:**
- [x] Tests pass: test unitario de parsing de config con vars custom → 8/8 (RED→GREEN)
- [x] Manual check: arrancar el binario con env vars custom y verlas reflejadas en el log → `bind 127.0.0.1:9090, 4096, 4 threads`; defaults `0.0.0.0:8080, 52428800, 6 threads`; inválido → exit 1

**Dependencies:** T1

**Files likely touched:**
- `src/config.rs`
- `src/main.rs`
- `.env.example`

**Estimated scope:** Small (2-3 archivos)

---

### Task 3: Esqueleto de 3 capas + `/health` + body limit + problem+json base (spike vertical)

**Description:** Crea la estructura de módulos `api/`, `app/`, `domain/` (esqueletos). Implementa el spike vertical mínimo: router axum con `GET /health`, capa `DefaultBodyLimit::max(50MB)`, y una versión base de `ProblemDetails` (RFC 9457) con `Content-Type: application/problem+json` para el caso 413 (body excedido). Community del contrato HTTP temprano.

**Acceptance criteria:**
- [x] `GET /health` responde `200 OK` con JSON mínimo
- [x] POST con body >50MB responde `413` con `Content-Type: application/problem+json` y cuerpo RFC 9457
- [x] La estructura `src/api/`, `src/app/`, `src/domain/` existe y compila

**Verification:**
- [x] Tests pass: `cargo test` (test de integración del health y del 413) → 2/2 (RED→GREEN)
- [x] Manual check: `curl -I -X POST localhost:8080/extract -d @grande.json` → `413 application/problem+json` → verificado con curl en puerto efímero

**Dependencies:** T1, T2

**Files likely touched:**
- `src/lib.rs`
- `src/main.rs`
- `src/api/{mod,router,handlers,problem_details}.rs`
- `src/app/{mod,state}.rs`
- `src/domain/mod.rs`

**Estimated scope:** Medium (6-8 archivos esqueleto + test)

---

## ✅ Checkpoint A (T1-T3)
- [x] `cargo build --release` compila sin warnings
- [x] Server arranca; `GET /health` → 200
- [x] POST body >50MB → 413 `application/problem+json`
- [ ] Revisar contrato HTTP con el humano antes de continuar

---

## Fase 1 — Capa de Dominio / Infraestructura

### Task 4: Modelos de dominio + errores del dominio

**Description:** Tipos de la capa de dominio: `PdfBytes` (bytes decodificados), `PageText`, `ExtractedDocument { page_count, pages, text }` y enum `DomainError` con variantes tipadas (base64 inválido, firma PDF inválida, parseo lopdf, extracción). Sin clones: los DTOs consumen por valor.

**Acceptance criteria:**
- [x] `DomainError` modela al menos: Base64Decode, InvalidPdfSignature, PdfParse, Extraction
- [x] `ExtractedDocument` no contiene `String` duplicadas ni referencias prestadas innecesarias → `text()` derivado de `pages` con pre-sizing, sin campo duplicado
- [x] `#[derive(Debug, PartialEq)]` en todos los tipos para tests

**Verification:**
- [x] Tests pass: `cargo test domain::model` (construcción/igualdad de tipos) → 5/5 (RED→GREEN)
- [x] Manual check: revisión de módulo por clippy sin warnings → `clippy -D warnings` limpio

**Dependencies:** T1

**Files likely touched:**
- `src/domain/model.rs`
- `src/domain/mod.rs`

**Estimated scope:** Small (1-2 archivos)

---

### Task 5: Decodificador Base64 SIMD con buffer exacto

**Description:** `domain::base64_decoder` que decodifica el payload con `base64_simd`. Pre-cálcula el tamaño exacto del buffer con `needed_bufsize` y asigna el `Vec<u8>` con esa capacidad antes de decodificar (cero reasignaciones). Devuelve `PdfBytes` o `DomainError::Base64Decode`.

**Acceptance criteria:**
- [x] Decodifica un Base64 válido (con padding) y devuelve exactamente los bytes originales
- [x] Base64 inválido → `DomainError::Base64Decode` (sin panic)
- [x] El buffer se pre-asigna con el tamaño calculado (`Vec::with_capacity`/`needed_bufsize`), verificable en benchmark/test de tamaño de capacidad

**Verification:**
- [x] Tests pass: unit tests con vectores conocidos (`b"Hello, World!"` → base64), caso inválido, caso con padding
- [x] Build: `cargo clippy -- -D warnings`

**Dependencies:** T4

**Files likely touched:**
- `src/domain/base64_decoder.rs`
- `src/domain/mod.rs`

**Estimated scope:** Small (1-2 archivos)

---

### Task 6: Parseo `lopdf` en memoria + validación `%PDF-`

**Description:** `domain::pdf_parser` que valida estructuralmente la firma mágica `%PDF-` en los primeros bytes y carga el documento con `lopdf::Document::load_mem(&bytes)` directamente desde el slice en RAM (sin tocar disco). Devuelve el `Document` o `DomainError::InvalidPdfSignature` / `DomainError::PdfParse`.

**Acceptance criteria:**
- [x] Sin firma `%PDF-` → `InvalidPdfSignature` (respuesta HTTP 400 en capa API)
- [x] PDF válido → `Document` parseado; `page_count == n` correcto
- [x] PDF corrupto → `PdfParse` (sin panic)

**Verification:**
- [x] Tests pass: fixture PDF generado válido + bytes sin firma + bytes corruptos
- [x] Manual check: `load_mem` nunca escribe a disco (solo `load_mem`; fixture generado en RAM)

**Dependencies:** T4

**Files likely touched:**
- `src/domain/pdf_parser.rs`
- `src/domain/pdf_utils.rs`
- `src/domain/mod.rs`
- `tests/fixtures/`

**Estimated scope:** Small (2-3 archivos + fixtures)

---

### Task 7: Motor de extracción paralela por páginas (rayon, N hilos auto-detectados)

**Description:** `domain::page_extractor` que itera las páginas del `Document` en paralelo con `rayon` (`(0..page_count).into_par_iter()`), extrae texto con `lopdf::Document::extract_text` por página dentro del pool dedicado, recolecta `PageText` y agrega el texto total con `String::with_capacity(estimación)` para evitar reasignaciones. Una página que falle devuelve texto vacío (aislamiento de errores).

**Acceptance criteria:**
- [x] Extrae texto de un PDF multipágina en paralelo; orden de páginas preservado
- [x] Texto total agregado en un único `String` pre-dimensionado (medible: sin duplicar páginas enteras en memoria) → usando `ExtractedDocument::text()` derivado con pre-sizing
- [x] Una página problemática no aborta la extracción de las demás

**Verification:**
- [x] Tests pass: fixture de 2+ páginas → nº de páginas, contenido esperado, orden
- [x] Bench/micro-check: uso de N hilos (N = núcleos del dispositivo) confirmado en log/bench (ver T12)
- [x] Manual check: revisión de ausencia de `.clone()` en la ruta de agregación

**Dependencies:** T5, T6

**Files likely touched:**
- `src/domain/page_extractor.rs`
- `src/domain/mod.rs`
- `tests/fixtures/multipage.pdf`

**Estimated scope:** Medium (3-4 archivos)

---

## ✅ Checkpoint B (T4-T7)
- [ ] `cargo test domain::*` todos verdes
- [ ] `cargo clippy -- -D warnings` y `cargo fmt --check` limpios
- [ ] Extracción correcta sobre fixture multipágina vía llamada directa al dominio
- [ ] Revisión humana: cero clones en ruta caliente

---

## Fase 2 — Capa de Aplicación / Servicio

### Task 8: Servicio extractor orquestador (`spawn_blocking` + `pool.install`)

**Description:** `app::extract_service` que orquesta la petición de extremo a extremo: mueve el payload a un `tokio::task::spawn_blocking`, instala el pool rayon dedicado (`ThreadPoolBuilder` con N hilos auto-detectados vía `available_parallelism()`, creado una vez en `AppState`), encadena decode → validar → parsear → extraer, y traduce `DomainError` al tipo de error de la capa API. Emite un span de tracing con duración, tamaño y page_count.

**Acceptance criteria:**
- [x] El metodo es `async` y delega todo el cómputo a `spawn_blocking` (sin bloquear el runtime async)
- [x] Usa el pool rayon compartido de `AppState` (no pool global)
- [x] Emite tracing span con `duration_ms`, `bytes`, `page_count`

**Verification:**
- [x] Tests pass: test que invoca el servicio contra un fixture y valida `ExtractedDocument` → 5/5 (RED→GREEN)
- [x] Manual check: log muestra el span con métricas (`bytes=1132 page_count=2 duration_ms=2`); el endpoint responde mientras hay carga (vía `spawn_blocking`, T10)

**Dependencies:** T2, T7

**Files likely touched:**
- `src/app/extract_service.rs`
- `src/app/state.rs`
- `src/app/mod.rs`
- `src/main.rs` (span CLOSE visible en log)

**Estimated scope:** Medium (3 archivos)

---

## ✅ Checkpoint C (T8)
- [x] El flujo completo (invocado por test/CLI) produce `ExtractedDocument` correcto
- [x] El runtime async no se bloquea (peticiones concurrentes siguen respondiendo)
- [x] Span de tracing con métricas visible en log

---

## Fase 3 — Capa de Presentación / API y Errores

### Task 9: ProblemDetails RFC 9457 completo

**Description:** `api::problem_details` completo: struct `ProblemDetails { type, title, status, detail, instance }` con `IntoResponse` que fija `Content-Type: application/problem+json` y el status HTTP correspondiente. Mapeo exhaustivo de `DomainError` → ProblemDetails (base64 inválido → 400, firma inválida → 400, parseo → 422, extracción → 500) además de 413 y 404.

**Acceptance criteria:**
- [x] Todas las respuestas de error llevan `Content-Type: application/problem+json` (incl. fallback 404)
- [x] Mapeo completo `DomainError → (status, title, detail)` documentado en el modulo (tabla en doc-comment)
- [x] Cuerpo JSON conforme a RFC 9457 (campos `type`, `title`, `status`, `detail`, `instance`)

**Verification:**
- [x] Tests pass: unit tests del mapping y del `IntoResponse` (headers + status + body) → 7/7 (RED→GREEN) + integración 404 → 3/3
- [x] Manual check: `curl -v` muestra el header en cada caso de error (via `from_domain` + fallback)

**Dependencies:** T3, T4

**Files likely touched:**
- `src/api/problem_details.rs`
- `src/api/mod.rs`
- `src/api/handlers.rs` (fallback `not_found`)
- `src/api/router.rs` (`.fallback`)
- `tests/api_integration.rs` (caso 404)

**Estimated scope:** Small (1-2 archivos)

---

### Task 10: Handler `POST /extract` definitivo + contrato de respuesta

**Description:** Handler definitivo que (1) valida la forma del body JSON con `serde` `{ document_base64: String }`, (2) lo entrega al servicio (T8), (3) responde `200` con `ExtractedDocument` serializado. Con `DefaultBodyLimit` 50MB. Contrato congelado en T3 debe confirmarse aquí.

**Acceptance criteria:**
- [x] `POST /extract` con PDF válido → `200` + JSON con `page_count`, `pages`, `text`, y `duration_ms`
- [x] Base64 malformado → `400 problem+json`; PDF sin firma → `400`; PDF corrupto → `422`
- [x] Body >50MB → `413 problem+json`

**Verification:**
- [x] Tests integración: casos 200, 400 base64, 400 firma, 422, 400 JSON inválido, 413 → 8/8 (RED→GREEN)
- [x] Manual check: `curl` con PDF de 200 páginas → `200 application/json`, `page_count=200`, `duration_ms=60`; span `extract{bytes=84700 page_count=200 duration_ms=60}` en log

**Dependencies:** T8, T9

**Files likely touched:**
- `src/api/handlers.rs`
- `src/api/contract.rs`
- `src/api/router.rs`
- `src/domain/test_support.rs` (accesible a integración para fixtures reproducibles)
- `tests/api_integration.rs`

**Estimated scope:** Small (3 archivos)

---

## ✅ Checkpoint D (T9-T10)
- [x] Todos los casos HTTP (200/400/413/422/404) funcionan con `problem+json` donde corresponde
- [x] Flujo end-to-end pasa con PDF de 200 páginas; logs con span de métricas
- [ ] Revisión humana del contrato de respuesta

---

## Fase 4 — Calidad, Rendimiento y Cierre

### Task 11: Tests de integración HTTP

**Description:** `tests/api_integration.rs` levanta la aplicación (vía `lib.rs`/`build_app`) en un puerto efímero y cubre el contrato completo HTTP: 200 (PDF válido y verificación del contenido extraído), 400 base64 inválido, 400 sin firma, 422 PDF corrupto, 413 body grande, y 404 ruta inexistente (con problem+json).

**Acceptance criteria:**
- [ ] Suite de integración corre contra la app real sin puerto en conflicto
- [ ] Todos los casos de la lista cubiertos, con asserts sobre status + headers `Content-Type` problem+json
- [ ] Fixtures generados de forma reproducible (o incluidos en `tests/fixtures/`)

**Verification:**
- [ ] Tests pass: `cargo test` (integración incluida)
- [ ] Build: `cargo clippy -- -D warnings`

**Dependencies:** T10

**Files likely touched:**
- `tests/api_integration.rs`
- `tests/fixtures/`
- `src/lib.rs`

**Estimated scope:** Medium (3-4 archivos)

---

### Task 12: Benchmarks (criterion): base64 decode + escalabilidad de extracción

**Description:** Benchmarks con `criterion`: (a) decodificación de ~50MB de base64 vs baseline; (b) extracción de un PDF grande (fixture de 200+ páginas) con 1, 2, 4, N/2 y N hilos (N = `available_parallelism()` del dispositivo) para documentar el speedup del pool rayon.

**Acceptance criteria:**
- [ ] Bench de decode reporta MB/s (valida vectorización SIMD y buffer exacto)
- [ ] Bench de escalabilidad muestra speedup 1→N hilos (objetivo: >~0.7·N de speedup en extracción)
- [ ] `criterion` configurado detrás de profile/feature (`cargo bench` no rompe `cargo test`)

**Verification:**
- [ ] Manual: `cargo bench` genera informe en `target/criterion/` y no falla
- [ ] Resultados documentados en README (T13)

**Dependencies:** T7

**Files likely touched:**
- `benches/extraction_bench.rs`
- `benches/base64_bench.rs`
- `Cargo.toml` (dev-deps: criterion)

**Estimated scope:** Medium (2-3 archivos)

---

### Task 13: README + docs de arquitectura + cierre

**Description:** `README.md` con: arquitectura de 3 capas, cómo compilar con el perfil release y `target-cpu=native` (incluye warning sobre compilar en la misma CPU de producción), contrato HTTP completo, tabla de errores RFC 9457, cómo correr/correjr tests y benchmarks. Verificación final de calidad del repo.

**Acceptance criteria:**
- [ ] README documenta stack, contrato, errores, build optimizado y pruebas
- [ ] Resultados de benchmarks incluidos como tablas
- [ ] `cargo fmt --check`, `cargo clippy -- -D warnings` y `cargo test` verdes

**Verification:**
- [ ] Manual: seguir el README desde cero permite compilar/correr/testear
- [ ] Revisión de PR con el humano

**Dependencies:** T11, T12

**Files likely touched:**
- `README.md`
- `Cargo.toml` (ajustes finales si aplica)

**Estimated scope:** Medium (2-3 archivos)

---

## ✅ Checkpoint Final (T1-T13)
- [ ] `cargo fmt --check` y `cargo clippy -- -D warnings` verdes
- [ ] `cargo test` completo (unit + integración) verde
- [ ] `cargo bench` produce informe; speedup 1→N hilos (N del dispositivo) documentado
- [ ] Contrato HTTP y tabla de errores aprobados por el humano
- [ ] Listo para revisión final y merge