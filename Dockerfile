# syntax=docker/dockerfile:1

FROM rust:1-slim AS builder

WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY .cargo .cargo
COPY src src
COPY benches benches
COPY tools tools

RUN cargo build --release

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends curl \
    && rm -rf /var/lib/apt/lists/*

ENV EXTRACT_BIND_ADDR=0.0.0.0:8000
ENV EXTRACT_BODY_LIMIT_BYTES=15728640
ENV EXTRACT_MAX_DECOMPRESSED_BYTES=67108864
ENV RUST_LOG=info

COPY --from=builder /build/target/release/extract /usr/local/bin/extract

RUN useradd --system --uid 10001 --create-home appuser
USER appuser

EXPOSE 8000

ENTRYPOINT ["/usr/local/bin/extract"]