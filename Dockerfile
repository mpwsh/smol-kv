FROM rust:1.73.0-slim-bookworm AS build
WORKDIR /build

RUN apt-get update -y && \
  apt-get install -y clang \
  && \
  rm -rf /var/lib/apt/lists/*

COPY src src
COPY Cargo.toml Cargo.lock ./

RUN cargo fetch --locked

RUN cargo build --locked --release
COPY scripts scripts
RUN scripts/strip-bins.sh target/release bin

FROM debian:bookworm-slim AS base
COPY --from=build build/bin/smol-kv bin/
EXPOSE 5050
CMD ["./bin/smol-kv"]
