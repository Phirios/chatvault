FROM node:25-alpine AS frontend_builder

WORKDIR /app

COPY --link ./frontend/package.json ./frontend/package-lock.json ./
RUN npm install
COPY --link ./frontend .
RUN npm run generate
RUN npm prune

FROM rust:1-bookworm AS backend_builder

WORKDIR /app/backend-rust

COPY ./backend-rust/Cargo.toml ./backend-rust/Cargo.lock* ./
RUN mkdir -p src && echo "fn main() {}" > src/main.rs && cargo build --release

COPY ./backend-rust/src ./src
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=frontend_builder /app/.output/public /app/public
COPY --from=backend_builder /app/backend-rust/target/release/chatvault /app/chatvault

ENV BIND=0.0.0.0:8080
ENV CHATVAULT_PUBLIC_DIR=/app/public
ENV CHATVAULT_SCRATCH_DIR=/tmp/chatvault
ENV CHATVAULT_BUCKET_PROVIDER=s3

EXPOSE 8080

ENTRYPOINT ["/app/chatvault"]
