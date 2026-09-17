# extract-text-microservice-pdf-extractext

Microservicio en Rust de alto rendimiento que recibe un PDF codificado en Base64 (200-500 páginas, ~12MB+), lo decodifica y extrae el texto de todas sus páginas en paralelo mediante `rayon`. Parsea el PDF en memoria con `lopdf` (sin tocar disco) y expone una API HTTP JSON (`axum` + `tokio`) con límite de body de 50MB y errores bajo RFC 9457 (Problem Details).

> **Estado de implementación:** T1 completado (scaffold del proyecto). El desglose de tareas y el plan viven en `tasks/todo.md` y `tasks/plan.md`.

## Stack

| Concern | Stack |
|---------|-------|
| Web | `axum` + `tokio` (body limit 50MB) |
| Decodificación Base64 | `base64-simd` (AVX2/NEON) |
| Parseo PDF | `lopdf` desde `&[u8]` en memoria |
| Paralelismo | `rayon` (pool dedicado, N hilos auto-detectados) |
| Serialización | `serde` + `serde_json` |
| Observabilidad | `tracing` + `tracing-subscriber` |

## Compilación

Perfil `release` optimizado con `lto="fat"`, `codegen-units=1` y `RUSTFLAGS="-C target-cpu=native"` (repo-local en `.cargo/config.toml`):

```sh
cargo build --release
```

> Compilar en la misma CPU donde correrá en producción para aprovechar la vectorización SIMD del chip destino.

## Estructura

- `src/` — código fuente (API / aplicación / dominio, 3 capas)
- `tests/` — tests de integración HTTP + fixtures
- `tasks/` — plan de implementación y task list

## Licencia

MIT