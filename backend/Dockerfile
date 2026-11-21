# Tahap build: kompilasi binary release Rust
FROM rust:1-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release

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
CMD ["psk-backend"]
