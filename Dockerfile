# syntax=docker/dockerfile:1.3-labs
FROM --platform=linux/amd64 rust:1.58.1-alpine3.15 as builder

LABEL org.opencontainers.image.source=https://github.com/haimgel/node-dns

# C compiler is needed for Ring, etc.
RUN apk add build-base && \
    adduser -u 1000 app -D && \
    mkdir -p /app /src && \
    chown app /src /app

USER app
COPY --chown=app . /src
WORKDIR /src
RUN --mount=type=cache,target=/usr/local/cargo/registry,uid=1000 \
    --mount=type=cache,target=/src/target,uid=1000 \
    cargo build --release && \
    cp /src/target/release/node-dns /app/node-dns

FROM alpine:3.15
RUN adduser -u 1000 app -D && \
    mkdir /app

COPY --from=builder /app/* /app
USER app
ENTRYPOINT ["/app/node-dns"]
