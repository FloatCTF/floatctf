#!/usr/bin/env bash
set -Eeuo pipefail

# FloatCTF AWD 边界验收 E2E：顺序执行 hardening 与 reset 两个场景。
#
# 复用 scripts/test-awd-business-e2e.sh 的全部脚手架（隔离 PostgreSQL/Redis/API 端口，
# 真实 floatctf-helper + Docker + WireGuard + nftables）。每个场景自行完成清理，
# 因此可以连续运行。baseline 主流程仍由 scripts/test-awd-business-e2e.sh 单独执行。

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

for scenario in hardening reset; do
    printf '\n=== AWD boundary scenario: %s ===\n' "$scenario" >&2
    AWD_E2E_SCENARIO="$scenario" "$ROOT/scripts/test-awd-business-e2e.sh"
done

printf '\nAWD boundary E2E: PASS (hardening + reset)\n' >&2
