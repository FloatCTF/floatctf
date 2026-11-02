FROM ubuntu:24.04

ARG FLOATCTF_VERSION=unknown
# 容器以 65532:${FLOATCTF_GID} 运行（GID 是宿主 floatctf 组的真实 gid，各机不同），
# 镜像内的组必须用同一个 gid 才不会出现"组名解析不到"。
ARG FLOATCTF_GID=65532

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        python3 \
        tzdata \
    && rm -rf /var/lib/apt/lists/*

COPY floatctf /usr/local/bin/floatctf
RUN chmod 0755 /usr/local/bin/floatctf \
    && mkdir -p /var/lib/floatctf/runtime \
    && chown 65532:65532 /var/lib/floatctf/runtime

# 为容器的数值身份建一个命名用户 floatctf：否则 Web 终端里的 shell 解析不到 uid，
# bash 会打印 "I have no name!"，且 HOME 缺失会让不少工具报错。组 gid 用宿主
# floatctf 组的真实 gid（构建参数传入），与 compose 的 ${FLOATCTF_UID:-65532}:${FLOATCTF_GID}
# 对齐；宿主上仍只创建 floatctf 组、不创建同名用户。
RUN groupadd -f --gid "${FLOATCTF_GID}" floatctf \
    && { getent passwd 65532 >/dev/null 2>&1 \
         || useradd --uid 65532 --gid "${FLOATCTF_GID}" --home-dir /var/lib/floatctf/runtime \
                    --shell /bin/bash --no-create-home floatctf; }

ENV HOME=/var/lib/floatctf/runtime

WORKDIR /var/lib/floatctf/runtime
USER 65532:65532

LABEL io.floatctf.managed="true" \
      org.opencontainers.image.title="FloatCTF API" \
      org.opencontainers.image.version="${FLOATCTF_VERSION}"

STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/floatctf"]
