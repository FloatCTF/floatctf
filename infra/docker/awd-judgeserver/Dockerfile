# FloatCTF AWD JudgeServer — production image (bookworm glibc base).
# The Rust binary is built against the Bookworm glibc baseline by
# scripts/build-runtime-images.sh; never copy a rolling-host binary here.
FROM python:3.12-slim-bookworm
COPY awd_judgeserver /usr/local/bin/awd_judgeserver
ENV LISTEN_ADDR=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/awd_judgeserver"]
