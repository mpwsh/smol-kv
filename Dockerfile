FROM rust:1.64.0-slim-bullseye AS build
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

FROM debian:bullseye-slim AS base
COPY --from=build build/bin/actix-rocksdb bin/
EXPOSE 5050
CMD ["./bin/actix-rocksdb"]
