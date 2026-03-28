# syntax=docker/dockerfile:1
FROM lukemathwalker/cargo-chef:latest-rust-1.89-bookworm AS chef
WORKDIR /app
RUN apt-get update -y && \
  apt-get install -y --no-install-recommends clang pkg-config libclang-dev && \
  rm -rf /var/lib/apt/lists/*

# ── Plan (hash dependencies for caching) ─────────────────────────
FROM chef AS planner
COPY Cargo.* ./
COPY rust-toolchain.toml rust-toolchain.toml
COPY src src
RUN cargo chef prepare --recipe-path recipe.json

# ── Build (cached deps layer + final binary) ─────────────────────
FROM chef AS builder
COPY rust-toolchain.toml rust-toolchain.toml
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY Cargo.* ./
COPY src src
RUN cargo build --release --bin smol-kv && \
  strip target/release/smol-kv

# ── Runtime ──────────────────────────────────────────────────────
FROM debian:bookworm-slim
RUN useradd --uid 10000 --create-home runner
WORKDIR /app
COPY --from=builder /app/target/release/smol-kv .
COPY web web
RUN chown -R runner:runner /app
EXPOSE 8080
USER 10000
ENTRYPOINT ["./smol-kv"]
