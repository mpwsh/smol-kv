FROM alpine:edge as build

WORKDIR /app

# Install build dependencies including build-base for full gcc toolchain
RUN apk --no-cache add \
    rust \
    cargo \
    build-base \
    openssl \
    openssl-dev \
    clang \
    clang16-libclang \
    jq \
    ca-certificates \
    bash \
    linux-headers \
    cmake \
    perl \
    git \
    zlib-dev \
    bzip2-dev \
    lz4-dev \
    snappy-dev \
    zstd-dev

# Set environment variables for static linking
ENV OPENSSL_STATIC=yes \
    PKG_CONFIG_ALLOW_CROSS=true \
    PKG_CONFIG_ALL_STATIC=true \
    RUSTFLAGS="-C target-feature=+crt-static" \
    CC=/usr/bin/gcc \
    CXX=/usr/bin/g++ \
    CFLAGS="-static" \
    CXXFLAGS="-static"

# Copy only dependency files first
COPY Cargo.toml Cargo.lock ./

# Now copy the real source code
COPY src src

# Build the application
RUN cargo build --release --target x86_64-alpine-linux-musl

# Create final minimal image
FROM scratch

WORKDIR /app
ENV PATH=/app:${PATH}

COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=build /app/target/x86_64-alpine-linux-musl/release/smol-kv /app

ENTRYPOINT ["/app/smol-kv"]
