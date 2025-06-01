# SPDX-License-Identifier: Apache-2.0
# Credits: The Typst Authors

FROM --platform=$BUILDPLATFORM tonistiigi/xx AS xx
FROM --platform=$BUILDPLATFORM rust:alpine AS build

COPY --from=xx / /

RUN apk add --no-cache clang lld
COPY . /app
WORKDIR /app
RUN --mount=type=cache,target=/root/.cargo/git/db \
    --mount=type=cache,target=/root/.cargo/registry/cache \
    --mount=type=cache,target=/root/.cargo/registry/index \
    CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse \
    cargo fetch

ARG TARGETPLATFORM

RUN xx-apk add --no-cache musl-dev openssl-dev openssl-libs-static
RUN --mount=type=cache,target=/root/.cargo/git/db \
    --mount=type=cache,target=/root/.cargo/registry/cache \
    --mount=type=cache,target=/root/.cargo/registry/index \
    OPENSSL_NO_PKG_CONFIG=1 OPENSSL_STATIC=1 \
    OPENSSL_DIR=$(xx-info is-cross && echo /$(xx-info)/usr/ || echo /usr) \
    xx-cargo build -p tytanic --release && \
    cp target/$(xx-cargo --print-target-triple)/release/tt target/release/tt && \
    xx-verify target/release/tt

FROM alpine:latest

ARG CREATED
ARG REVISION

LABEL org.opencontainers.image.authors="The Tytanic Project Developers"
LABEL org.opencontainers.image.created=${CREATED}
LABEL org.opencontainers.image.description="A test runner for typst projects."
LABEL org.opencontainers.image.documentation="https://tingerrr.github.io/tytanic/"
LABEL org.opencontainers.image.licenses="MIT OR Apache-2.0"
LABEL org.opencontainers.image.revision=${REVISION}
LABEL org.opencontainers.image.source="https://github.com/tingerrr/tytanic"
LABEL org.opencontainers.image.title="Tytanic Docker image"
LABEL org.opencontainers.image.vendor="tytanic"

COPY --from=build  /app/target/release/tt /bin
ENTRYPOINT [ "/bin/tt" ]
