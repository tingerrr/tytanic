FROM rust:1.80 AS builder

WORKDIR /usr/src/typst-test
COPY . .
RUN cargo install --path crates/typst-test-cli

FROM debian:bullseye-slim

RUN \
    apt-get update \
        && apt-get openssl-dev openssl-libs \
        && rm -rf /var/lib/apt/lists/*

COPY \
    --from=builder \
    /usr/local/cargo/bin/tt \
    /usr/local/bin/tt

CMD ["typst-test"]
