#!/usr/bin/env bash
set -Eeuo pipefail

# FloatCTF 全服务验收入口。
#
# 覆盖：
#   - 基础设施：PostgreSQL / Redis / RustFS / Caddy / floatctf-helper
#   - 服务镜像：AWD FlagServer / AWD JudgeServer / AWDP JudgeServer
#   - 后端：Rust workspace tests
#   - 前端：Vitest / TypeScript / production build
#   - 真实业务 E2E：Jeopardy 三模式 / AWD / AWDP 三模式
#
# 所有业务 E2E 均使用隔离 PostgreSQL + Redis + API 进程，但会真实使用宿主
# helper、Docker、WireGuard/nftables（AWD）和开发 RustFS。为避免两个 API 的
# scheduler/worker 抢同一全局资源，本脚本要求普通开发 API (:9090) 未运行。

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PROJECT_ROOT="${PROJECT_ROOT:-$ROOT}"

RUN_CORE=1
RUN_E2E=1
BUILD_IMAGES=1
START_INFRA=0
KEEP_GOING=0

usage() {
    cat <<'USAGE'
Usage: scripts/test-all-services.sh [options]

Options:
  --e2e-only       只跑基础设施检查 + 三套真实业务 E2E
  --core-only      只跑基础设施检查 + Rust/Web 测试，不跑真实业务 E2E
  --skip-images    不重建 AWD/AWDP 服务镜像（要求本地已有最新镜像）
  --start-infra    测试前执行 scripts/infra-up.sh
  --keep-going     某阶段失败后继续跑后续独立阶段，最后统一返回失败
  -h, --help       显示帮助

默认执行完整验收：infra → service images → Rust → Web → Jeopardy → AWD → AWDP。
USAGE
}

while (($#)); do
    case "$1" in
        --e2e-only) RUN_CORE=0 ;;
        --core-only) RUN_E2E=0 ;;
        --skip-images) BUILD_IMAGES=0 ;;
        --start-infra) START_INFRA=1 ;;
        --keep-going) KEEP_GOING=1 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done

need() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "[all-services] missing command: $1" >&2
        exit 2
    }
}
for cmd in bash cargo curl docker pnpm psql python3 ss systemctl; do need "$cmd"; done

RUN_ID="$(date +%Y%m%d-%H%M%S)-$$"
LOG_DIR="${FLOATCTF_ALL_SERVICES_LOG_DIR:-/tmp/floatctf-all-services-$RUN_ID}"
mkdir -p "$LOG_DIR"

STAGE_NAMES=()
STAGE_STATUS=()
STAGE_SECONDS=()
FAILED=0

record_stage() {
    STAGE_NAMES+=("$1")
    STAGE_STATUS+=("$2")
    STAGE_SECONDS+=("$3")
}

print_summary() {
    printf '\n=== FLOATCTF ALL SERVICES SUMMARY ===\n'
    local i
    for i in "${!STAGE_NAMES[@]}"; do
        printf '%-34s %-5s %4ss\n' "${STAGE_NAMES[$i]}" "${STAGE_STATUS[$i]}" "${STAGE_SECONDS[$i]}"
    done
    printf 'logs: %s\n' "$LOG_DIR"
    if ((FAILED)); then
        printf 'RESULT: FAIL\n'
    else
        printf 'RESULT: PASS\n'
    fi
}
trap print_summary EXIT

run_stage() {
    local name=$1
    shift
    local slug start elapsed rc
    slug="$(printf '%s' "$name" | tr ' /:' '---' | tr -cd '[:alnum:]_.-')"
    start=$SECONDS
    printf '\n[all-services] >>> %s\n' "$name"
    set +e
    "$@" 2>&1 | tee "$LOG_DIR/$slug.log"
    rc=${PIPESTATUS[0]}
    set -e
    elapsed=$((SECONDS - start))
    if ((rc == 0)); then
        record_stage "$name" PASS "$elapsed"
        printf '[all-services] PASS %s (%ss)\n' "$name" "$elapsed"
        return 0
    fi
    record_stage "$name" FAIL "$elapsed"
    FAILED=1
    printf '[all-services] FAIL %s (%ss), log=%s\n' "$name" "$elapsed" "$LOG_DIR/$slug.log" >&2
    if ((KEEP_GOING)); then
        return 0
    fi
    exit "$rc"
}

assert_container_healthy() {
    local name=$1 state health
    state="$(docker inspect -f '{{.State.Status}}' "$name" 2>/dev/null || true)"
    [[ "$state" == running ]] || { echo "$name is not running (state=$state)" >&2; return 1; }
    health="$(docker inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{end}}' "$name" 2>/dev/null || true)"
    if [[ -n "$health" && "$health" != healthy ]]; then
        echo "$name is not healthy (health=$health)" >&2
        return 1
    fi
}

preflight() {
    [[ "$(systemctl is-active floatctf-helper.service 2>/dev/null || true)" == active ]] || {
        echo 'floatctf-helper.service is not active' >&2; return 1;
    }
    [[ -S /run/floatctf/helper-control.sock ]] || { echo 'missing helper-control.sock' >&2; return 1; }
    [[ -S /run/floatctf/helper-docker.sock ]] || { echo 'missing helper-docker.sock' >&2; return 1; }

    assert_container_healthy floatctf-dev-db
    assert_container_healthy floatctf-dev-redis
    assert_container_healthy floatctf-dev-rustfs
    assert_container_healthy floatctf-dev-caddy

    PGPASSWORD=postgres psql -X -q -A -t -h 127.0.0.1 -U postgres -d floatctf_db -c 'SELECT 1' | grep -qx '1'
    docker exec floatctf-dev-redis redis-cli ping | grep -qx PONG
    docker exec floatctf-dev-rustfs sh -c 'nc -z 127.0.0.1 9000'

    # Caddy 的 /private 反代必须能到 RustFS。对象不存在允许 4xx，但不能连接失败/502。
    local caddy_status
    caddy_status="$(curl -sS --max-time 3 -o /dev/null -w '%{http_code}' http://127.0.0.1:7780/private/__floatctf_all_services_probe__ || true)"
    [[ "$caddy_status" != 000 && "$caddy_status" != 502 ]] || {
        echo "Caddy -> RustFS proxy unavailable (HTTP $caddy_status)" >&2; return 1;
    }

    # 普通开发 API 会运行 AWDP 常驻 scheduler，可能与隔离 E2E 的 practice judge 竞争。
    if ss -ltnH | awk '{print $4}' | grep -Eq '(^|:)9090$'; then
        echo 'port 9090 is already listening; stop the normal dev API before full-service E2E' >&2
        return 1
    fi

    # Jeopardy E2E cleanup historically按这些运行时名清理，因此先拒绝已有业务实例，绝不误删。
    if docker ps -a --format '{{.Names}}' | grep -Eq '^(JP|JS|JT)-'; then
        echo 'existing Jeopardy runtime containers found; refusing destructive E2E cleanup' >&2
        return 1
    fi
    if docker ps -aq --filter label=awd.event_id | grep -q .; then
        echo 'existing AWD-labelled containers found' >&2; return 1
    fi
    if docker network ls --format '{{.Name}}' | grep -q '^fctf-awd-'; then
        echo 'existing AWD event network found' >&2; return 1
    fi
    if ip -o link show 2>/dev/null | awk -F': ' '{print $2}' | grep -q '^fawg_'; then
        echo 'existing AWD WireGuard interface found' >&2; return 1
    fi
    if docker ps -a --format '{{.Names}}' | grep -Eq '^fctf-awdp-judge-|^awdp-'; then
        echo 'existing AWDP competition runtime found' >&2; return 1
    fi
    if docker ps -a --format '{{.Names}}' | grep -qx 'fctf-awdp-practice-judge'; then
        echo 'existing AWDP practice judge found; stop the dev API / clean practice judge first' >&2
        return 1
    fi
    if docker ps -a --format '{{.Names}}' | grep -Eq '^floatctf-(jeopardy|awd|awdp)-.*e2e-redis-'; then
        echo 'stale E2E Redis container found; clean it before running all-services' >&2
        return 1
    fi

    local active_awd
    active_awd="$(PGPASSWORD=postgres psql -X -q -A -t -h 127.0.0.1 -U postgres -d floatctf_db \
        -c "SELECT count(*) FROM awd_events WHERE status::text NOT IN ('finished','archived')" 2>/dev/null || echo '?')"
    [[ "$active_awd" == 0 ]] || {
        echo "development DB has active AWD rows ($active_awd)" >&2; return 1;
    }

    echo "helper=active db=healthy redis=healthy rustfs=healthy caddy=reachable"
}

build_service_images() {
    bash scripts/build-runtime-images.sh
}

verify_service_images() {
    local image
    local binary
    for image in \
        floatctf/awd-flagserver:latest \
        floatctf/awd-judgeserver:latest \
        floatctf/infra/awdp-judgeserver:latest; do
        docker image inspect "$image" >/dev/null 2>&1 || {
            echo "missing required service image: $image" >&2
            return 1
        }
        case "$image" in
            floatctf/awd-flagserver:latest) binary=/usr/local/bin/awd_flagserver ;;
            floatctf/awd-judgeserver:latest) binary=/usr/local/bin/awd_judgeserver ;;
            floatctf/infra/awdp-judgeserver:latest) binary=/usr/local/bin/awdp_judgeserver ;;
        esac
        # This catches both missing shared libraries and GLIBC symbol-version
        # mismatches before any business E2E starts.
        docker run --rm --entrypoint sh "$image" -ec             "ldd '$binary' >/tmp/ldd.txt 2>&1; cat /tmp/ldd.txt; ! grep -Eq 'not found|GLIBC_[0-9.]+.*not found' /tmp/ldd.txt"
        docker image inspect "$image" --format "$image {{.Id}} {{.Size}}"
    done
}

if ((START_INFRA)); then
    run_stage 'infra up' bash scripts/infra-up.sh
fi
run_stage 'infrastructure preflight' preflight

if ((BUILD_IMAGES)); then
    run_stage 'service images build' build_service_images
else
    run_stage 'service images verify' verify_service_images
fi

if ((RUN_CORE)); then
    run_stage 'Rust workspace tests' bash scripts/test-rust.sh
    run_stage 'Web unit tests' pnpm --filter @floatctf/web test
    run_stage 'Web TypeScript' pnpm --filter @floatctf/web exec tsc --noEmit
    run_stage 'Web production build' pnpm --filter @floatctf/web build
fi

if ((RUN_E2E)); then
    # 三套 E2E 使用同一个 API binary。过去每个子脚本都会再调用一次 cargo build；
    # 统一预构建一次，减少大型 floatctf crate 的重复链接/历史 hash artifact。
    run_stage 'E2E API build' cargo build -p floatctf

    # 必须串行：三套验收共享 helper / Docker / host networking 等全局控制面。
    run_stage 'Jeopardy real HTTP E2E' env FLOATCTF_E2E_SKIP_API_BUILD=1 bash scripts/test-jeopardy-http-e2e.sh
    run_stage 'AWD real business E2E' env FLOATCTF_E2E_SKIP_API_BUILD=1 bash scripts/test-awd-business-e2e.sh
    run_stage 'AWDP real HTTP E2E' env FLOATCTF_E2E_SKIP_API_BUILD=1 bash scripts/test-awdp-http-e2e.sh
fi

exit "$FAILED"
