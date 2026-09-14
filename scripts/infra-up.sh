#!/usr/bin/env bash
set -Eeuo pipefail

: "${PROJECT_ROOT:?PROJECT_ROOT 未设置，请使用 mise dev 或 source ./activate.sh}"

RUSTFS_LOG_DIR="$PROJECT_ROOT/app/logs/rustfs"
COMPOSE_FILE="$PROJECT_ROOT/infra/compose/compose.dev.yml"
RUSTFS_IMAGE="rustfs/rustfs:1.0.0-beta.12"
RUSTFS_VOLUME="floatctf-dev-rustfs-data"

# rustfs 容器以当前开发者 uid/gid 运行（compose.dev.yml 的 user:），
# 使写入的日志/对象文件直接归属开发者——宿主无需 sudo 即可读。
# Caddy 使用容器 stdout/stderr，不维护宿主文件日志目录。
DOCKER_UID="$(id -u)"
DOCKER_GID="$(id -g)"
export DOCKER_UID DOCKER_GID

# 用 root 容器执行一次属主对齐（幂等）。bind 目录旧属主可能是容器 uid 10001
#（历史方案 chown 10001:10001:750：容器可写但开发者自己不可读），也可能是
# root（install.sh --develop 以 root 建目录）；开发者无权直接 chown，须借容器。
chown_as_root() { # $1 = 宿主路径（bind 目录）或 named volume 名
    docker run --rm --user 0 \
        -v "$1":/target \
        --entrypoint sh "$RUSTFS_IMAGE" \
        -c "chown -R $(id -u):$(id -g) /target" >/dev/null 2>&1
}

# ── 日志目录：属主必须是当前开发者（容器同 uid 写入，文件直接归开发者）──────
mkdir -p "$PROJECT_ROOT/app/logs"
if [ -d "$RUSTFS_LOG_DIR" ]; then
    owner="$(stat -c '%u' "$RUSTFS_LOG_DIR" 2>/dev/null || echo 0)"
    if [ "$owner" != "$(id -u)" ]; then
        echo "日志目录属主为 uid $owner（历史容器属主），对齐为当前用户..."
        if chown_as_root "$RUSTFS_LOG_DIR"; then
            echo "日志目录属主已对齐（旧日志文件同时变为可读）"
        else
            echo "[WARN] 日志目录属主对齐失败；若 rustfs 写日志报 EACCES 请重试: $RUSTFS_LOG_DIR" >&2
        fi
    fi
fi
mkdir -p "$RUSTFS_LOG_DIR"

# ── 启动基础设施 ─────────────────────────────────────────────────────────────
# 先不 --wait：fresh named volume 会按 RustFS 镜像内 /data 的 uid=10001 初始化，
# 而开发容器刻意以当前开发者 uid/gid 运行。若这里直接 --wait，RustFS 会先因
# EACCES 进入 unhealthy，随后即使属主修好了，本次命令也已经被判失败。
UP_STATUS=0
docker compose -f "$COMPOSE_FILE" up -d --build "$@" || UP_STATUS=1

# ── rustfs 数据卷属主对齐 ────────────────────────────────────────────────────
# volume 已由第一阶段创建；用 root 容器把属主对齐到开发者，再重启 RustFS。
# dev 数据体量小，chown 秒级；无条件执行保证幂等，不依赖属主探测。
if docker volume inspect "$RUSTFS_VOLUME" >/dev/null 2>&1; then
    if chown_as_root "$RUSTFS_VOLUME"; then
        docker compose -f "$COMPOSE_FILE" restart rustfs >/dev/null 2>&1 \
            || echo "[WARN] rustfs 重启失败，请检查容器日志"
    else
        echo "[WARN] rustfs 数据卷属主对齐失败；若容器报 EACCES 请重跑本命令" >&2
        UP_STATUS=1
    fi
fi

# 第二阶段再统一等待健康状态；这样 fresh RustFS volume 不会产生“先 unhealthy、
# 修好后仍返回失败”的假阴性，同时端口占用/其他服务 unhealthy 仍会正确失败。
if [ "$UP_STATUS" -eq 0 ]; then
    docker compose -f "$COMPOSE_FILE" up -d --wait "$@" || UP_STATUS=1
fi

exit "$UP_STATUS"
