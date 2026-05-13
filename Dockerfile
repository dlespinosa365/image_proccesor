# syntax=docker/dockerfile:1.7

# ---- Stage 1: builder con cache de dependencias ----
FROM rust:1-slim-bookworm AS builder

WORKDIR /app

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
        ca-certificates \
 && rm -rf /var/lib/apt/lists/*

COPY Cargo.* ./

RUN mkdir -p src \
 && echo "fn main() { println!(\"placeholder\"); }" > src/main.rs \
 && cargo build --release \
 && rm -rf src \
        target/release/deps/image_proccesor* \
        target/release/image_proccesor*

COPY src ./src

RUN cargo build --release \
 && strip target/release/image_proccesor || true

# ---- Stage 2: runtime mínimo ----
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        ca-certificates \
        tini \
        wget \
 && rm -rf /var/lib/apt/lists/* \
 && useradd -r -u 1001 -m -d /home/appuser appuser \
 && mkdir -p /data/images \
 && chown -R appuser:appuser /data

COPY --from=builder /app/target/release/image_proccesor /usr/local/bin/image_proccesor

USER appuser

ENV BIND_ADDR=0.0.0.0:8080 \
    OUTPUT_DIR=/data/images \
    RUST_LOG=info

EXPOSE 8080
VOLUME ["/data/images"]

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/usr/local/bin/image_proccesor"]
