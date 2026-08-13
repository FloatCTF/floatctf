# FloatCTF AWDP Practice JudgeServer.
# Build with the repository root as the Docker context.
FROM rust:1.97.1-slim-bookworm AS builder
WORKDIR /workspace
ENV CARGO_HOME=/usr/local/cargo

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY . .
RUN cargo build --release -p floatctf-awdp-judgeserver

FROM python:3.12-slim-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /workspace/target/release/awdp_judgeserver /usr/local/bin/awdp_judgeserver
ENV LISTEN_ADDR=0.0.0.0:8082
EXPOSE 8082
ENTRYPOINT ["/usr/local/bin/awdp_judgeserver"]
