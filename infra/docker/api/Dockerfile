FROM ubuntu:24.04

ARG FLOATCTF_VERSION=unknown

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        tzdata \
    && rm -rf /var/lib/apt/lists/*

COPY floatctf /usr/local/bin/floatctf
RUN chmod 0755 /usr/local/bin/floatctf \
    && mkdir -p /var/lib/floatctf/runtime \
    && chown 65532:65532 /var/lib/floatctf/runtime

WORKDIR /var/lib/floatctf/runtime
USER 65532:65532

LABEL io.floatctf.managed="true" \
      org.opencontainers.image.title="FloatCTF API" \
      org.opencontainers.image.version="${FLOATCTF_VERSION}"

STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/floatctf"]
