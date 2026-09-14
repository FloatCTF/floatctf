# FloatCTF AWDP JudgeServer — production image.
# Build the binary against the Bookworm baseline via scripts/build-runtime-images.sh;
# never copy a binary built on a rolling host distro into this image.
FROM python:3.12-slim-bookworm
COPY awdp_judgeserver /usr/local/bin/awdp_judgeserver
ENV DATA_LISTEN_ADDR=0.0.0.0:80
EXPOSE 80
ENTRYPOINT ["/usr/local/bin/awdp_judgeserver"]
