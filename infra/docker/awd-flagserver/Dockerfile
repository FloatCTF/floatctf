# FloatCTF AWD FlagServer — production image (bookworm glibc base).
# The Rust binary is built against the Bookworm glibc baseline by
# scripts/build-runtime-images.sh; never copy a rolling-host binary here.
#
# 不用 python 基础镜像：FlagServer 只做 HTTP 服务，从不执行选手脚本。
# 体积从 ~187 MB 降到 ~116 MB。
FROM debian:bookworm-slim
COPY awd_flagserver /usr/local/bin/awd_flagserver
ENV LISTEN_ADDR=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/awd_flagserver"]
