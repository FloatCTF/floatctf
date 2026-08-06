#!/usr/bin/env bash
# =============================================================================
# FloatCTF API 测试脚本
#
# 用法（在任意目录）：
#   bash src/floatctf-api/scripts/run_api_tests.sh
# 或：
#   cd src/floatctf-api && ./scripts/run_api_tests.sh
#
# 环境变量（可选）：
#   FLOATCTF_API_BASE          API 地址，默认 http://127.0.0.1:8080
#   FLOATCTF_API_REQUIRE=1     API 不可达时 HTTP 测试失败（默认 soft-skip）
#   FLOATCTF_TEST_USER         选手账号
#   FLOATCTF_TEST_PASS         选手密码
#   FLOATCTF_TEST_ADMIN        超管账号
#   FLOATCTF_TEST_ADMIN_PASS   超管密码
#   UNIT_ONLY=1                只跑单元测试，不碰 HTTP
#   HTTP_ONLY=1                只跑 HTTP 测试
#
# 示例：
#   # 仅单元（无需起服务）
#   UNIT_ONLY=1 ./scripts/run_api_tests.sh
#
#   # 起好 API 后做全量
#   export FLOATCTF_API_BASE=http://127.0.0.1:8080
#   export FLOATCTF_TEST_USER=demo
#   export FLOATCTF_TEST_PASS='your-pass'
#   export FLOATCTF_TEST_ADMIN=admin
#   export FLOATCTF_TEST_ADMIN_PASS='admin-pass'
#   ./scripts/run_api_tests.sh
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
API_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$API_ROOT"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

ok()   { echo -e "${GREEN}✔${NC} $*"; }
warn() { echo -e "${YELLOW}!${NC} $*"; }
err()  { echo -e "${RED}✘${NC} $*"; }

FLOATCTF_API_BASE="${FLOATCTF_API_BASE:-http://127.0.0.1:8080}"
export FLOATCTF_API_BASE

echo "=============================================="
echo " FloatCTF API tests"
echo "  cwd:     $API_ROOT"
echo "  API:     $FLOATCTF_API_BASE"
echo "  UNIT_ONLY=${UNIT_ONLY:-0}  HTTP_ONLY=${HTTP_ONLY:-0}"
echo "  REQUIRE=${FLOATCTF_API_REQUIRE:-0}"
echo "=============================================="
echo

FAILED=0

run_unit() {
  echo "---- [1/3] 单元测试：JWT + DynamicScore ----"
  if cargo test jwt_roundtrip -- --nocapture; then
    ok "JWT unit tests"
  else
    err "JWT unit tests failed"
    FAILED=1
  fi
  if cargo test dynamic_score -- --nocapture; then
    ok "DynamicScore unit tests"
  else
    err "DynamicScore unit tests failed"
    FAILED=1
  fi
  echo
}

run_http() {
  echo "---- [2/3] HTTP 鉴权契约（全 protected 路由无 Token 须拒绝）----"
  if cargo test --test http_auth_contract -- --nocapture; then
    ok "http_auth_contract"
  else
    err "http_auth_contract failed"
    FAILED=1
  fi
  echo

  echo "---- [3/3] HTTP 业务冒烟（需账号 env 才深测）----"
  if [[ -n "${FLOATCTF_TEST_USER:-}" && -n "${FLOATCTF_TEST_PASS:-}" ]]; then
    ok "已设置 FLOATCTF_TEST_USER（将跑用户侧 flow）"
  else
    warn "未设置 FLOATCTF_TEST_USER/PASS → 用户侧 flow 会 skip"
  fi
  if [[ -n "${FLOATCTF_TEST_ADMIN:-}" && -n "${FLOATCTF_TEST_ADMIN_PASS:-}" ]]; then
    ok "已设置 FLOATCTF_TEST_ADMIN（将跑管理侧 flow）"
  else
    warn "未设置 FLOATCTF_TEST_ADMIN/PASS → 管理侧 flow 会 skip"
  fi

  if cargo test --test http_flow -- --nocapture; then
    ok "http_flow"
  else
    err "http_flow failed"
    FAILED=1
  fi
  echo
}

if [[ "${HTTP_ONLY:-0}" != "1" ]]; then
  run_unit
else
  warn "HTTP_ONLY=1，跳过单元测试"
fi

if [[ "${UNIT_ONLY:-0}" != "1" ]]; then
  run_http
else
  warn "UNIT_ONLY=1，跳过 HTTP 测试"
fi

echo "=============================================="
if [[ "$FAILED" -eq 0 ]]; then
  ok "全部完成（exit 0）"
  echo
  echo "提示：若 HTTP 显示 skip，说明 API 未就绪或返回 502。"
  echo "  1) 先启动 floatctf-api"
  echo "  2) 确认 FLOATCTF_API_BASE 端口正确"
  echo "  3) 再设账号 env 后重跑"
  exit 0
else
  err "存在失败（exit 1）"
  exit 1
fi
