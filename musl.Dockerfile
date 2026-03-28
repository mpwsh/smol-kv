FROM alpine:3.21 AS build
WORKDIR /app
RUN apk --no-cache add \
  curl gcc g++ musl-dev openssl-dev openssl-libs-static \
  clang clang-libclang linux-headers ca-certificates bash pkgconf

# Install Rust via rustup (not Alpine's packaged rust which uses a non-standard target triple)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.89.0
ENV PATH="/root/.cargo/bin:${PATH}"

ENV OPENSSL_STATIC=yes \
  PKG_CONFIG_ALLOW_CROSS=true \
  PKG_CONFIG_ALL_STATIC=true \
  RUSTFLAGS="-C target-feature=+crt-static"

COPY Cargo.toml Cargo.lock ./
COPY src src

# Build for the host — on Alpine that's musl-linked, no --target needed
RUN CC=/usr/bin/gcc CXX=/usr/bin/g++ cargo build --release && \
  strip target/release/smol-kv

FROM scratch
WORKDIR /app
ENV PATH=/app:${PATH}
COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=build /app/target/release/smol-kv /app/smol-kv
ENTRYPOINT ["/app/smol-kv"]
