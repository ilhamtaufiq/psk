# syntax=docker/dockerfile:1
# Tahap build: kompilasi binary release Rust
FROM rust:1-bookworm AS build
WORKDIR /app

# 1) Dependensi saja. Layer ini hanya dibangun ulang bila Cargo.toml atau Cargo.lock berubah,
#    jadi perubahan kode tidak memicu kompilasi ulang seluruh crate pihak ketiga.
COPY Cargo.toml Cargo.lock ./
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    mkdir -p src && echo "fn main() {}" > src/main.rs \
    && cargo build --release \
    && rm -rf src target/release/psk-backend target/release/deps/psk_backend-*

# 2) Kode aplikasi. Hanya crate ini yang dikompilasi ulang.
COPY src ./src
COPY migrations ./migrations
# touch: memaksa cargo membangun ulang crate meski mtime file sumber lebih lama dari cache
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    touch src/main.rs && cargo build --release

# Tahap runtime: image kecil tanpa toolchain
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/psk-backend /usr/local/bin/psk-backend

ENV BIND_ADDR=0.0.0.0:8080 \
    DATABASE_URL=sqlite:///data/app.db \
    UPLOAD_DIR=/data/uploads \
    RUST_LOG=info

# Database dan berkas proyek disimpan di /data (pasang sebagai volume persisten)
VOLUME ["/data"]
EXPOSE 8080
# Cek kesehatan: endpoint /api/health harus 200 (database terbaca)
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["psk-backend", "healthcheck"]

CMD ["psk-backend"]
