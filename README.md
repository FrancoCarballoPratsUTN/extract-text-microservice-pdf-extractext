# extract-text-microservice-pdf-extractext

Microservicio en Rust de alto rendimiento que recibe un PDF crudo (200-500 páginas, hasta 15MB), lo valida y extrae el texto de todas sus páginas en paralelo mediante un pool dedicado de `rayon`. Parsea el PDF en memoria con `lopdf` (sin tocar disco) y expone una API HTTP (`axum` + `tokio`) con límite de body de 15MB y errores bajo RFC 9457 (Problem Details). Contrato binario **v2**: el body del `POST /extract` es el PDF, sin base64.

> **Estado de implementación:** completo (66 tests: 44 unit + 17 integración + 5 de paridad, incluyendo paridad byte-a-byte con el sample real de 250 páginas; gate de rendimiento P95 ≤ 400ms en contenedor). El extractor `lean` (scanner de content-stream de pasada única) es el predeterminado y `lopdf` queda disponible como rollback. El desglose de tareas y el plan viven en `tasks/todo.md` y `tasks/plan.md`.

## Stack

| Concern | Stack |
|---------|-------|
| Web | `axum` + `tokio` (body limit 15MB, 413 problem+json) |
| Contrato | `POST /extract` binario (PDF crudo, sin base64) |
| Parseo PDF | `lopdf` desde `&[u8]` en memoria |
| Paralelismo | `rayon` (pool dedicado, N hilos auto-detectados) |
| Extractor | `lean` (árbol de páginas resuelto una vez); rollback con `EXTRACT_EXTRACTOR=lopdf` |
| Serialización | `serde` + `serde_json` |
| Observabilidad | `tracing` + `tracing-subscriber` |

## Compilación

Perfil `release` optimizado con `lto="fat"` y `codegen-units=1`. El binario
se compila con instrucciones portables para poder ejecutarse en cualquier CPU;
el pool de Rayon detecta en runtime todos los CPUs permitidos al proceso:

```sh
cargo build --release
```

El extractor predeterminado es `lean`. Para diagnosticar o revertir temporalmente
al extractor de referencia:

```sh
EXTRACT_EXTRACTOR=lopdf cargo run --release
```

La cantidad de workers puede sobrescribirse con `EXTRACT_NUM_THREADS`, pero por
defecto se detecta automáticamente mediante `available_parallelism()`.

## Estructura

- `src/` — código fuente (API / aplicación / dominio, 3 capas)
- `tests/` — tests de integración HTTP + fixtures
- `tasks/` — plan de implementación y task list

## Licencia

MIT