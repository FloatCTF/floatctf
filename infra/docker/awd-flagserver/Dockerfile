# FloatCTF AWD FlagServer — production image (bookworm glibc base).
# The Rust binary is built for a glibc-2.34 baseline in a rust:1.97.1-slim-bookworm
# container by scripts/build-release.sh; alpine cannot run the glibc binary.
FROM python:3.12-slim-bookworm
COPY awd_flagserver /usr/local/bin/awd_flagserver
ENV LISTEN_ADDR=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/awd_flagserver"]
