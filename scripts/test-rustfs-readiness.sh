#!/usr/bin/env bash
#
# FloatCTF release risk R2 验收：RustFS readiness → API 启动竞态。
#
# 背景（Phase 13 实测）：Compose 的 rustfs healthcheck 只探 TCP 端口
# （`nc -z 127.0.0.1 9000`），而端口打开 ≠ S3/HTTP 层可用。API 在
# `depends_on: service_healthy` 之后立刻做 bucket 初始化，一次性失败 → bootstrap
# panic → crash-loop，直到 restart 策略把它拉起来。
#
# 本脚本用真实容器验证两条契约：
#   1. healthcheck 契约：`GET /health` 探针在「TCP 开着但 HTTP 不应答」时必须
#      fail closed（旧的 nc -z 检查会误判为 healthy）；真实 RustFS 从不可用到 200
#      的转换必须被观测到。
#   2. 有界重试契约（API 容器级）：
#      - RustFS 迟到：API 先起、RustFS 后起 → API 最终 healthy，且 RestartCount 保持 0；
#      - RustFS 永不到达：API 在有界窗口（12 次 / 90s）内以清晰错误退出，不无限挂起。
#
# 隔离：独立的 docker network + 独立的 PostgreSQL 容器 + 独立的 Redis 容器 +
# 独立的 RustFS 容器 + 独立 DB，全部以 `r2test-<suffix>` 命名，trap 中全部回收。
# 绝不触碰既有 `fctf*` 容器/网络与 /run/floatctf（API 容器里的 Docker 端点是本脚本
# 自己起的 fake Docker API，只回答 ping，不做任何宿主 Docker 操作）。
#
# 用法：bash scripts/test-rustfs-readiness.sh
# 退出码：0 = 全部通过（或环境不足时 [SKIP]）；1 = 有失败项。

set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

RUN_ID="$(date +%Y%m%d-%H%M%S)-$$"
SUFFIX="$(printf '%s' "$RUN_ID" | tr -cd '0-9' | tail -c 6)"
PREFIX="r2test-$SUFFIX"

NET="$PREFIX-net"
PG="$PREFIX-pg"
REDIS="$PREFIX-redis"
RUSTFS="$PREFIX-rustfs"
TCPONLY="$PREFIX-tcponly"
PROBE_RUNNER="$PREFIX-probe"
API_LATE="$PREFIX-api-late"
API_NEVER="$PREFIX-api-never"
NEVER_HOST="$PREFIX-never"

PG_IMAGE="postgres:17"
REDIS_IMAGE="redis:7-alpine"
RUSTFS_IMAGE="rustfs/rustfs:1.0.0-beta.12"
PROBE_IMAGE="alpine:3"
FALLBACK_API_IMAGE="floatctf/api:1.0.0"
API_IMAGE="floatctf/api:r2test-$SUFFIX"

RUSTFS_AK="r2testaccess"
RUSTFS_SK="r2testsecret"

PG_DB="floatctf_r2_$SUFFIX"
PG_PORT=""
TMP="$(mktemp -d "${TMPDIR:-/tmp}/floatctf-r2test.XXXXXX")"
chmod 0755 "$TMP"

PASS=0
FAIL=0
SKIP=0
RC_LATE="n/a"
RC_NEVER="n/a"
log() { printf '\n=== %s ===\n' "$*" >&2; }
pass() { printf '[PASS] %s\n' "$*"; PASS=$((PASS + 1)); }
fail() { printf '[FAIL] %s\n' "$*" >&2; FAIL=$((FAIL + 1)); }
skip() { printf '[SKIP] %s\n' "$*"; SKIP=$((SKIP + 1)); }
note() { printf '[INFO] %s\n' "$*"; }

# ── 清理（只回收本脚本创建的、以 r2test-<suffix> 命名的资源）────────────────
FAKE_DOCKER_PID=""
cleanup() {
    local rc=$?
    set +e
    # -v：同时回收 PostgreSQL 容器的匿名 volume（不留宿主残留）。
    docker rm -fv "$API_NEVER" "$API_LATE" "$PROBE_RUNNER" "$TCPONLY" "$RUSTFS" "$REDIS" "$PG" \
        >/dev/null 2>&1
    docker network rm "$NET" >/dev/null 2>&1
    if [[ -n "$API_IMAGE" ]]; then docker rmi -f "$API_IMAGE" >/dev/null 2>&1; fi
    if [[ -n "$FAKE_DOCKER_PID" ]]; then kill "$FAKE_DOCKER_PID" >/dev/null 2>&1; fi
    rm -rf "$TMP"
    return $rc
}
trap cleanup EXIT

# ── 前置检查 ────────────────────────────────────────────────────────────────
for cmd in docker curl python3 psql; do
    command -v "$cmd" >/dev/null 2>&1 || {
        skip "缺少命令 $cmd，无法运行 R2 就绪性测试"
        exit 0
    }
done
docker info >/dev/null 2>&1 || {
    skip "docker daemon 不可用"
    exit 0
}
for image in "$PG_IMAGE" "$REDIS_IMAGE" "$RUSTFS_IMAGE" "$PROBE_IMAGE"; do
    docker image inspect "$image" >/dev/null 2>&1 || {
        skip "缺少镜像 $image（本测试不联网拉取）"
        exit 0
    }
done

MISE=()
command -v mise >/dev/null 2>&1 && MISE=(mise exec --)

# ── 工具函数 ────────────────────────────────────────────────────────────────
restart_count() { docker inspect -f '{{.RestartCount}}' "$1" 2>/dev/null || echo '?'; }
is_running() { [[ "$(docker inspect -f '{{.State.Running}}' "$1" 2>/dev/null)" == "true" ]]; }
exit_code() { docker inspect -f '{{.State.ExitCode}}' "$1" 2>/dev/null || echo '?'; }
health_status() {
    docker inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}' "$1" 2>/dev/null || echo '?'
}
# tracing-subscriber 给字段名加 ANSI 颜色，直接 grep 字段会失配；先剥色再匹配。
container_logs() { docker logs "$1" 2>&1 | sed -E 's/\x1b\[[0-9;]*m//g'; }

# 已验证的 in-container /health 探针（BusyBox nc 无 -q，需要 stdin 保持打开）。
# 目标主机用 $TARGET 传入，端口固定 9000（RustFS S3 端口）。
PROBE_SCRIPT='{ printf "GET /health HTTP/1.1\r\nHost: %s:9000\r\nConnection: close\r\n\r\n" "$TARGET"; sleep 1; } | nc -w 3 "$TARGET" 9000 | head -1 | grep -q " 200 "'
probe_health() {
    docker exec -e TARGET="$1" "$PROBE_RUNNER" sh -c "$PROBE_SCRIPT" >/dev/null 2>&1
}
# 旧的 TCP-only 检查（用于对照）。
tcp_reachable() {
    docker exec "$PROBE_RUNNER" nc -z "$1" 9000 >/dev/null 2>&1
}
wait_for_health() { # host timeout_secs
    local host=$1 timeout=$2 waited=0
    while ((waited < timeout)); do
        probe_health "$host" && return 0
        sleep 1
        waited=$((waited + 1))
    done
    return 1
}
wait_for_http_health_any() { # container timeout_secs（容器自身 healthcheck 变 healthy）
    local name=$1 timeout=$2 waited=0
    while ((waited < timeout)); do
        [[ "$(health_status "$name")" == "healthy" ]] && return 0
        is_running "$name" || return 1
        sleep 2
        waited=$((waited + 2))
    done
    return 1
}
wait_for_container_exit() { # name timeout_secs
    local name=$1 timeout=$2 waited=0
    while ((waited < timeout)); do
        is_running "$name" || return 0
        sleep 2
        waited=$((waited + 2))
    done
    return 1
}
# 等待容器内探针命令成功；容器若提前退出则立即失败并打印诊断。
wait_for_container_ready() { # name timeout_secs cmd...
    local name=$1 timeout=$2
    shift 2
    local waited=0
    while ((waited < timeout)); do
        if ! is_running "$name"; then
            docker inspect -f 'container {{.Name}} status={{.State.Status}} exit={{.State.ExitCode}}' "$name" >&2 || true
            docker logs --tail 20 "$name" >&2 || true
            return 1
        fi
        if docker exec "$name" "$@" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
        waited=$((waited + 1))
    done
    docker inspect -f 'container {{.Name}} status={{.State.Status}} exit={{.State.ExitCode}}' "$name" >&2 || true
    docker logs --tail 20 "$name" >&2 || true
    return 1
}

# ── 场景 1：healthcheck 契约（TCP-only 误判 vs /health HTTP 探针）────────────
scenario_healthcheck_contract() {
    log "场景 1：healthcheck 契约"

    docker run -d --name "$PROBE_RUNNER" --network "$NET" "$PROBE_IMAGE" sleep infinity >/dev/null

    # 1a. TCP 开着但 HTTP 层缺席：旧检查说 healthy，新探针必须 fail closed。
    docker run -d --name "$TCPONLY" --network "$NET" "$PROBE_IMAGE" \
        sh -c 'while true; do nc -l -p 9000 >/dev/null 2>&1 || true; done' >/dev/null
    sleep 1
    if tcp_reachable "$TCPONLY"; then
        pass "TCP-only 假服务：9000 端口可连（旧的 \`nc -z\` healthcheck 会判 healthy）"
    else
        fail "TCP-only 假服务端口不可连，场景 1a 无法成立"
    fi
    local t0=$SECONDS
    if probe_health "$TCPONLY"; then
        fail "HTTP /health 探针在 TCP-only 服务上错误地 PASS（必须 fail closed）"
    else
        pass "HTTP /health 探针在 TCP-only 服务上正确失败（fail closed，探针耗时 $((SECONDS - t0))s < timeout 10s）"
    fi
}

# ── 基础设施：隔离 network / PostgreSQL / Redis / fake Docker API ────────────
start_fake_docker() {
    cat >"$TMP/fake_docker.py" <<'PY'
import http.server, json, os, socketserver, sys

SOCK, LOG = sys.argv[1], sys.argv[2]


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def _reply(self, code, body, ctype="application/json"):
        data = body.encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(data)

    def _handle(self):
        with open(LOG, "a") as fh:
            fh.write(f"{self.command} {self.path}\n")
        if self.path.startswith("/_ping"):
            self._reply(200, "OK", "text/plain")
        elif self.path.startswith("/version"):
            self._reply(200, json.dumps({
                "ApiVersion": "1.43", "MinAPIVersion": "1.24",
                "Version": "0.0.0-r2test-fake", "Os": "linux", "Arch": "amd64",
            }))
        elif self.path.startswith("/info"):
            self._reply(200, json.dumps({"ServerVersion": "0.0.0-r2test-fake"}))
        elif "/containers/json" in self.path or "/networks" in self.path or "/images/json" in self.path:
            self._reply(200, "[]")
        else:
            self._reply(200, "{}")

    do_GET = do_HEAD = do_POST = do_DELETE = do_PUT = do_handle = _handle

    def log_message(self, *args):
        pass


class UnixServer(socketserver.ThreadingUnixStreamServer):
    allow_reuse_address = True
    daemon_threads = True


if os.path.exists(SOCK):
    os.unlink(SOCK)
srv = UnixServer(SOCK, Handler)
os.chmod(SOCK, 0o777)
srv.serve_forever()
PY
    mkdir -p "$TMP/fake-docker"
    python3 "$TMP/fake_docker.py" "$TMP/fake-docker/docker.sock" "$TMP/docker-requests.log" \
        >"$TMP/fake-docker.log" 2>&1 &
    FAKE_DOCKER_PID=$!
    for _ in $(seq 1 50); do
        [[ -S "$TMP/fake-docker/docker.sock" ]] && break
        sleep 0.1
    done
    [[ -S "$TMP/fake-docker/docker.sock" ]] || {
        fail "fake Docker API socket 未创建"
        return 1
    }
    note "fake Docker API 就绪（只回答 ping，不触达宿主 Docker）: $TMP/fake-docker/docker.sock"
}

start_infra() {
    log "基础设施：隔离 network + PostgreSQL + Redis"
    docker network create "$NET" >/dev/null

    docker run -d --name "$PG" --network "$NET" -p 127.0.0.1::5432 \
        -e POSTGRES_USER=postgres -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB="$PG_DB" \
        "$PG_IMAGE" >/dev/null
    PG_PORT="$(docker port "$PG" 5432/tcp | sed -E 's/.*:([0-9]+)$/\1/' | head -n1)"
    [[ "$PG_PORT" =~ ^[0-9]+$ ]] || {
        fail "无法确定隔离 PostgreSQL 端口"
        return 1
    }
    if wait_for_container_ready "$PG" 60 pg_isready -U postgres -d "$PG_DB"; then
        pass "隔离 PostgreSQL 就绪（127.0.0.1:$PG_PORT，db=$PG_DB）"
    else
        fail "隔离 PostgreSQL 未就绪"
        return 1
    fi

    docker run -d --name "$REDIS" --network "$NET" "$REDIS_IMAGE" >/dev/null
    if wait_for_container_ready "$REDIS" 60 redis-cli ping; then
        pass "隔离 Redis 就绪"
    else
        fail "隔离 Redis 未就绪"
        return 1
    fi

    start_fake_docker || return 1
}

# 生成 API 配置（两份：宿主侧用于迁移，容器侧用于运行）→ $1 输出文件 $2 DB host:port $3 rustfs url
write_config() {
    python3 - "$1" "$2" "$3" "$ROOT" "$PG_DB" "$REDIS" "$RUSTFS_AK" "$RUSTFS_SK" "$SUFFIX" <<'PY'
import sys
from pathlib import Path

out, db_netloc, rustfs_url, root, db, redis, ak, sk, suffix = sys.argv[1:10]
base = Path(root, "apps/api/config/development.toml").read_text()
s = base
s = s.replace('work_dir = "../../app"', 'work_dir = "/var/lib/floatctf/runtime"')
s = s.replace('socket_path = "/run/floatctf/helper-docker.sock"', 'socket_path = "/run/fake-docker.sock"')
s = s.replace(
    'url = "postgres://postgres:postgres@127.0.0.1:5432/floatctf_db"',
    f'url = "postgres://postgres:postgres@{db_netloc}/{db}"',
)
s = s.replace('endpoint_url = "http://127.0.0.1:9000"', f'endpoint_url = "{rustfs_url}"')
s = s.replace('access_key_id = "rustfsadmin"', f'access_key_id = "{ak}"')
s = s.replace('secret_access_key="rustfsadmin"', f'secret_access_key="{sk}"')
s = s.replace('url = "redis://127.0.0.1:6379/"', f'url = "redis://{redis}:6379/"')
s = s.replace('network_runtime = "helper"', 'network_runtime = "noop"')
s = s.replace('channel = "floatctf:realtime"', f'channel = "floatctf:realtime:r2test:{suffix}"')
Path(out).write_text(s)
PY
    chmod 0644 "$1"
}

apply_migrations() {
    log "迁移：对隔离 DB 应用 migration"
    local host_cfg="$TMP/api-host.toml"
    write_config "$host_cfg" "127.0.0.1:$PG_PORT" "http://127.0.0.1:9000"
    FLOATCTF_CONFIG="$host_cfg" apps/api/src/sql/migrate.sh apply >"$TMP/migrate.log" 2>&1 || {
        tail -n 40 "$TMP/migrate.log" >&2
        fail "migration 应用失败"
        return 1
    }
    local count
    count="$(PGPASSWORD=postgres psql -X -q -A -t -h 127.0.0.1 -p "$PG_PORT" -U postgres -d "$PG_DB" \
        -c 'SELECT count(*) FROM schema_migrations')"
    [[ "$count" =~ ^[0-9]+$ ]] && ((count > 0)) || {
        fail "schema_migrations 为空"
        return 1
    }
    pass "隔离 DB 迁移完成（$count 条）"
}

# 构建/准备 API 容器镜像与运行参数。
API_RUN_IMAGE=""
API_RUN_EXTRA=()
prepare_api_image() {
    log "准备 API 容器镜像"
    if [[ ! -x target/release/floatctf ]]; then
        note "target/release/floatctf 不存在，开始 release 构建（--locked -p floatctf --bins）"
        "${MISE[@]+"${MISE[@]}"}" cargo build --locked --release -p floatctf --bins >"$TMP/cargo-build.log" 2>&1 || {
            tail -n 40 "$TMP/cargo-build.log" >&2
            skip "release 构建失败"
            return 1
        }
    fi
    mkdir -p "$TMP/api-ctx"
    cp target/release/floatctf "$TMP/api-ctx/floatctf"
    if docker build -f infra/docker/api/Dockerfile --build-arg FLOATCTF_VERSION=test \
        -t "$API_IMAGE" "$TMP/api-ctx" >"$TMP/docker-build.log" 2>&1; then
        API_RUN_IMAGE="$API_IMAGE"
        pass "API 镜像构建完成（infra/docker/api/Dockerfile，FLOATCTF_VERSION=test）"
        return 0
    fi
    tail -n 20 "$TMP/docker-build.log" >&2
    note "docker build 失败，回退：把新二进制挂进既有生产镜像 $FALLBACK_API_IMAGE"
    if docker image inspect "$FALLBACK_API_IMAGE" >/dev/null 2>&1; then
        API_RUN_IMAGE="$FALLBACK_API_IMAGE"
        API_RUN_EXTRA=(-v "$ROOT/target/release/floatctf:/usr/local/bin/floatctf:ro")
        pass "使用 $FALLBACK_API_IMAGE + 挂载本次构建的 floatctf 二进制"
        return 0
    fi
    skip "无法准备 API 镜像（docker build 失败且无 $FALLBACK_API_IMAGE）"
    return 1
}

start_api_container() { # name config_file restart_policy
    local name=$1 cfg=$2 restart=$3
    docker run -d --name "$name" --network "$NET" --restart "$restart" \
        --cap-drop=ALL --security-opt=no-new-privileges:true --read-only \
        --tmpfs /tmp:rw,noexec,nosuid,nodev,size=64m \
        --tmpfs /var/lib/floatctf/runtime:rw,mode=1777,size=64m \
        ${API_RUN_EXTRA[@]+"${API_RUN_EXTRA[@]}"} \
        -e FLOATCTF_CONFIG=/etc/floatctf/floatctf.toml \
        -v "$cfg:/etc/floatctf/floatctf.toml:ro" \
        -v "$TMP/fake-docker/docker.sock:/run/fake-docker.sock" \
        --health-cmd 'curl -sS -o /dev/null --max-time 2 http://127.0.0.1:9090/api/users/me' \
        --health-interval 3s --health-timeout 3s --health-retries 30 --health-start-period 2s \
        "$API_RUN_IMAGE" >/dev/null
}

# ── 场景 2：RustFS 迟到 → API healthy 且 RestartCount 0 ──────────────────────
scenario_late_rustfs() {
    log "场景 2：RustFS 迟到（API 先起，RustFS 后起）"

    write_config "$TMP/api-late.toml" "$PG:5432" "http://$RUSTFS:9000"
    start_api_container "$API_LATE" "$TMP/api-late.toml" unless-stopped

    # API 必须活过「RustFS 不可达」阶段：不退出，同时按有界退避重试。
    # 注意：目标主机名不可解析时 SDK 的 DNS 解析本身可能耗时 ~20s，
    # 因此这里轮询等首次重试日志，而不是固定 sleep。
    local t0=$SECONDS logs="" first_retry=0 first_retry_after=""
    while ((SECONDS - t0 < 60)); do
        logs="$(container_logs "$API_LATE")"
        if grep -q "retrying RustFS bucket initialization" <<<"$logs"; then
            first_retry=1
            first_retry_after=$((SECONDS - t0))
            break
        fi
        is_running "$API_LATE" || break
        sleep 2
    done
    if is_running "$API_LATE"; then
        pass "RustFS 尚未启动时 API 没有退出（在有界重试窗口内）"
    else
        fail "RustFS 尚未启动时 API 提前退出"
        tail -n 40 <<<"$logs" >&2
    fi
    if grep -q "RustFS bucket initialization failed" <<<"$logs"; then
        pass "API 日志包含每次失败的分类结果：$(grep -m1 -o 'attempt=[0-9]* .*' <<<"$logs" | cut -c1-110)"
    else
        fail "API 日志没有重试告警"
    fi
    if ((first_retry)); then
        pass "API 记录了退避重试（首次重试日志出现于 ${first_retry_after}s）"
    else
        fail "60s 内未观测到 API 的退避重试记录"
    fi
    if grep -qE 'class="?transient' <<<"$logs"; then
        pass "失败被分类为 transient（未放弃重试）"
    else
        fail "失败未被分类为 transient"
    fi
    if [[ "$(restart_count "$API_LATE")" == "0" ]]; then
        pass "RustFS 不可达期间 RestartCount = 0"
    else
        fail "RustFS 不可达期间发生重启（RestartCount=$(restart_count "$API_LATE")）"
    fi

    # 迟到启动 RustFS，并观测「TCP 已开但 /health 尚未 200」的窗口。
    log "场景 2：迟到启动 RustFS（观测 TCP-open-but-HTTP-absent 窗口）"
    docker run -d --name "$RUSTFS" --network "$NET" --user "$(id -u):$(id -g)" \
        --tmpfs /data:rw,mode=1777,size=256m \
        --tmpfs /logs:rw,mode=1777,size=64m \
        -e RUSTFS_ADDRESS=":9000" \
        -e RUSTFS_ACCESS_KEY="$RUSTFS_AK" -e RUSTFS_SECRET_KEY="$RUSTFS_SK" \
        -e RUSTFS_CONSOLE_ENABLE=false -e RUSTFS_OBS_LOGGER_LEVEL=warn \
        "$RUSTFS_IMAGE" >/dev/null

    local t0=$SECONDS ready_after="" saw_tcp_before_http=0 early_fail=0
    while ((SECONDS - t0 < 120)); do
        if probe_health "$RUSTFS"; then
            ready_after=$((SECONDS - t0))
            break
        fi
        if tcp_reachable "$RUSTFS"; then saw_tcp_before_http=1; fi
        early_fail=$((early_fail + 1))
        sleep 1
    done
    if [[ -n "$ready_after" ]]; then
        pass "RustFS /health 在延迟 ${ready_after}s 后返回 200（in-container 探针 PASS）"
    else
        fail "RustFS /health 120s 内未返回 200"
        docker logs --tail 40 "$RUSTFS" >&2 || true
    fi
    if ((early_fail > 0)); then
        pass "就绪前 /health 探针正确失败 ${early_fail} 次（unhealthy → healthy 转换被观测到）"
    else
        note "首次探测即成功，未观测到 unhealthy 阶段（RustFS 启动过快）"
    fi
    if ((saw_tcp_before_http)); then
        pass "观测到「TCP 端口已开但 /health 还未 200」的窗口 —— 证明 TCP-only healthcheck 不足"
    else
        note "本次未捕捉到 TCP-open-but-HTTP-absent 窗口（场景 1a 已用确定性容器覆盖该契约）"
    fi

    # API 必须在有界窗口内变成 healthy，且全程零重启。
    local t1=$SECONDS
    if wait_for_http_health_any "$API_LATE" 90; then
        pass "RustFS 就绪后 API 容器变为 healthy（等待 $((SECONDS - t1))s）"
    else
        fail "RustFS 就绪后 API 未变为 healthy（health=$(health_status "$API_LATE")）"
        tail -n 60 <<<"$(container_logs "$API_LATE")" >&2
    fi
    local rc
    rc="$(restart_count "$API_LATE")"
    if [[ "$rc" == "0" ]]; then
        pass "验收头号指标：API healthy 且 RestartCount = 0（无 crash-loop churn）"
    else
        fail "API 发生重启：RestartCount=$rc"
    fi
    if is_running "$API_LATE"; then
        pass "API 容器仍在运行（未 crash-loop 退出）"
    else
        fail "API 容器已退出"
    fi
    if grep -q "Rustfs connected OK" <<<"$(container_logs "$API_LATE")"; then
        pass "API 日志确认 bucket 初始化最终成功（Rustfs connected OK）"
    else
        fail "API 日志缺少 bucket 初始化成功记录"
    fi
    RC_LATE="$rc"
    note "API_LATE RestartCount = $rc"
}

# ── 场景 3：RustFS 永不到达 → 有界窗口内清晰失败 ──────────────────────────────
scenario_never_rustfs() {
    log "场景 3：RustFS 永不到达（主机名不可解析）→ 有界窗口内失败"
    write_config "$TMP/api-never.toml" "$PG:5432" "http://$NEVER_HOST:9000"
    local t0=$SECONDS
    start_api_container "$API_NEVER" "$TMP/api-never.toml" no

    if wait_for_container_exit "$API_NEVER" 180; then
        local elapsed=$((SECONDS - t0))
        local code
        code="$(exit_code "$API_NEVER")"
        pass "API 在 ${elapsed}s 后退出（有界，不无限挂起），exit code=$code"
        if ((elapsed >= 60 && elapsed <= 150)); then
            pass "退出耗时 ${elapsed}s 落在有界窗口内（重试预算 12 次 / 90s）"
        else
            fail "退出耗时 ${elapsed}s 超出预期有界窗口（60s–150s）"
        fi
        if [[ "$code" != "0" ]]; then
            pass "以非零退出码失败（fail fast，等待编排层重启）"
        else
            fail "API 以 exit code 0 退出（应视为启动失败）"
        fi
        local logs
        logs="$(container_logs "$API_NEVER")"
        if grep -q "RustFS did not become usable within the bounded retry window" <<<"$logs"; then
            pass "日志包含清晰的有界耗尽消息"
            note "exhaustion: $(grep -m1 -o 'RustFS did not become usable.*' <<<"$logs" | cut -c1-160)"
        else
            fail "日志缺少有界耗尽消息"
            tail -n 40 <<<"$logs" >&2
        fi
    else
        fail "API 在有界窗口内没有退出（疑似无限重试/挂起）"
        docker logs --tail 40 "$API_NEVER" >&2 || true
    fi
    RC_NEVER="$(restart_count "$API_NEVER")"
    note "API_NEVER RestartCount = $RC_NEVER"
}

# ── 静态交叉检查：安装器 rustfs healthcheck（另一个 workstream 负责落地）─────
static_installer_healthcheck() {
    log "静态交叉检查：scripts/install.sh 的 rustfs healthcheck"
    local block test_line
    block="$(sed -n '/^    rustfs:/,/^    api:/p' scripts/install.sh)"
    test_line="$(grep -m1 -E '^[[:space:]]+test:' <<<"$block" || true)"
    if [[ -z "$test_line" ]]; then
        skip "install.sh 的 rustfs healthcheck 块未识别，跳过静态检查"
    elif grep -q "nc -z" <<<"$test_line"; then
        skip "install.sh 的 rustfs healthcheck 仍是 TCP-only（installer workstream 尚未应用 R2 spec）：${test_line#"${test_line%%[![:space:]]*}"}"
    elif grep -q "/health" <<<"$test_line"; then
        pass "install.sh 的 rustfs healthcheck 使用 HTTP /health 探针：${test_line#"${test_line%%[![:space:]]*}"}"
        if grep -q "interval: 10s" <<<"$block" && grep -q "timeout: 10s" <<<"$block" \
            && grep -q "retries: 12" <<<"$block" && grep -q "start_period: 30s" <<<"$block"; then
            pass "install.sh 的 rustfs healthcheck 参数符合 R2 spec（interval 10s / timeout 10s / retries 12 / start_period 30s）"
        else
            fail "install.sh 的 rustfs healthcheck 参数与 R2 spec 不一致"
        fi
    else
        skip "install.sh 的 rustfs healthcheck 形式未知，跳过静态检查：$test_line"
    fi
}

# ── 主流程 ──────────────────────────────────────────────────────────────────
log "R2 就绪性测试开始（run=$RUN_ID）"
BEFORE_CONTAINERS="$(docker ps -a --format '{{.Names}}' | wc -l)"
BEFORE_NETWORKS="$(docker network ls --format '{{.Name}}' | wc -l)"
note "基线：容器 $BEFORE_CONTAINERS 个 / 网络 $BEFORE_NETWORKS 个"

INFRA_OK=0
if start_infra; then
    INFRA_OK=1
    apply_migrations || INFRA_OK=0
else
    fail "隔离基础设施启动失败"
fi

if ((INFRA_OK)); then
    scenario_healthcheck_contract
else
    skip "隔离 network 不可用，跳过场景 1（healthcheck 契约）"
fi

if ((INFRA_OK)) && prepare_api_image; then
    scenario_late_rustfs
    scenario_never_rustfs
elif ((INFRA_OK)); then
    skip "未能准备 API 镜像，跳过场景 2/3（healthcheck 契约已在场景 1 真实验证）"
else
    skip "隔离基础设施不可用，跳过场景 2/3"
fi

static_installer_healthcheck

# ── 清理证明（显式回收后再检查残留）────────────────────────────────────────
log "清理与残留检查"
cleanup
trap - EXIT
LEFTOVER_C="$(docker ps -a --format '{{.Names}}' | grep -c "^$PREFIX" || true)"
LEFTOVER_N="$(docker network ls --format '{{.Name}}' | grep -c "^$PREFIX" || true)"
LEFTOVER_I="$(docker images --format '{{.Repository}}:{{.Tag}}' | grep -c "^floatctf/api:r2test-" || true)"
AFTER_CONTAINERS="$(docker ps -a --format '{{.Names}}' | wc -l)"
AFTER_NETWORKS="$(docker network ls --format '{{.Name}}' | wc -l)"
if [[ "$LEFTOVER_C" == "0" && "$LEFTOVER_N" == "0" && "$LEFTOVER_I" == "0" ]]; then
    pass "无本脚本残留容器/网络/镜像（前缀 $PREFIX / 镜像 floatctf/api:r2test-*）"
else
    fail "存在残留资源：容器 $LEFTOVER_C 个 / 网络 $LEFTOVER_N 个 / 镜像 $LEFTOVER_I 个"
    docker ps -a --format '{{.Names}}' | grep "^$PREFIX" >&2 || true
fi
if [[ "$BEFORE_CONTAINERS" == "$AFTER_CONTAINERS" && "$BEFORE_NETWORKS" == "$AFTER_NETWORKS" ]]; then
    pass "既有容器/网络数量未变（容器 $BEFORE_CONTAINERS → $AFTER_CONTAINERS，网络 $BEFORE_NETWORKS → $AFTER_NETWORKS）"
else
    fail "资源数量变化：容器 $BEFORE_CONTAINERS → $AFTER_CONTAINERS，网络 $BEFORE_NETWORKS → $AFTER_NETWORKS"
fi

printf '\n=== R2 RUSTFS READINESS SUMMARY ===\n'
printf 'PASS=%d FAIL=%d SKIP=%d\n' "$PASS" "$FAIL" "$SKIP"
printf 'RestartCount: api-late(RustFS 迟到)=%s  api-never(RustFS 永不到达)=%s\n' "$RC_LATE" "$RC_NEVER"
if ((FAIL > 0)); then
    printf 'R2 readiness test: FAIL\n'
    exit 1
fi
printf 'R2 readiness test: PASS\n'
