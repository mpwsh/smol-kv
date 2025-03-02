FROM rust:1.82.0-bullseye AS chef
RUN cargo install cargo-chef --locked

WORKDIR /app
RUN apt-get update -y && \
  apt-get install -y --no-install-recommends clang && \
  rm -rf /var/lib/apt/lists/*

FROM chef AS planner

COPY Cargo.* ./
COPY src src
COPY rust-toolchain.toml rust-toolchain.toml
# Compute a lock-like file for our project
RUN cargo chef prepare  --recipe-path recipe.json

FROM chef AS builder

COPY --from=planner /app/recipe.json recipe.json
# Build our project dependencies, not our application!
RUN cargo chef cook --release --recipe-path recipe.json

COPY Cargo.* ./
COPY src src

# Build our project
RUN cargo build --release

FROM debian:bookworm-slim AS runtime

COPY --from=builder /app/target/release/smol-kv smolkv
ENTRYPOINT ["./smolkv"]
