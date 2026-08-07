# FloatCTF AWD FlagServer.
# Build with the repository root as the Docker context.
FROM rust:1.97.1-slim-bookworm AS builder
WORKDIR /workspace
ENV CARGO_HOME=/usr/local/cargo

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY . .
RUN cargo build --release -p floatctf-awd-flagserver

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /workspace/target/release/awd_flagserver /usr/local/bin/awd_flagserver
ENV LISTEN_ADDR=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/awd_flagserver"]
