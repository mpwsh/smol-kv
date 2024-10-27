FROM alpine:edge AS build

WORKDIR /app
RUN apk --no-cache add rust cargo g++ openssl openssl-dev clang jq ca-certificates bash linux-headers clang16-libclang

ENV OPENSSL_STATIC=yes \
  PKG_CONFIG_ALLOW_CROSS=true \
  PKG_CONFIG_ALL_STATIC=true \
  RUSTFLAGS="-C target-feature=+crt-static"

COPY Cargo.toml Cargo.toml
COPY Cargo.lock Cargo.lock
COPY src src


RUN CC=/usr/bin/gcc CXX=/usr/bin/g++ cargo build --release --target x86_64-alpine-linux-musl

FROM scratch
WORKDIR /app
ENV PATH=/app:${PATH}

COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=build /app/target/x86_64-alpine-linux-musl/release/smol-kv /app

ENTRYPOINT ["/app/smol-kv"]
