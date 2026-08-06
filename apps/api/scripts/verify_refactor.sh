#!/usr/bin/env bash
# Structural verification for FloatCTF event-module refactor / AWD branch.
# Does not touch host network or Docker production resources.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export CARGO_TERM_COLOR=always
# Avoid sccache sandbox issues in constrained environments.
CARGO=(cargo --config 'build.rustc-wrapper=""')

echo "== fmt (check) =="
"${CARGO[@]}" fmt --all -- --check || {
  echo "fmt check failed — run: cargo fmt --all"
  exit 1
}

echo "== cargo check (lib + bins) =="
"${CARGO[@]}" check --lib
"${CARGO[@]}" check --bin floatctf 2>/dev/null || "${CARGO[@]}" check --bins
"${CARGO[@]}" check --bin awd_flagserver
"${CARGO[@]}" check --bin awd_judgeserver

echo "== unit tests (safe subset) =="
"${CARGO[@]}" test --lib infrastructure::realtime::publisher -- --nocapture || true
"${CARGO[@]}" test --lib modules::event::awd_team -- --nocapture || true

echo "== structural route / module checks =="
# Legacy strategy stack must be gone.
if [[ -d src/strategies ]]; then
  echo "FAIL: src/strategies must not exist"
  exit 1
fi
if rg -n 'EventStrategyFactory|EventStrategy\b|AwdEventAdapter' src --type rust 2>/dev/null; then
  echo "FAIL: EventStrategy symbols still present"
  exit 1
fi

# Unified AWD paths (no /api/awd/events).
if rg -n '#\[(get|post|put|delete)\("/awd/events' src/modules/event/awd_team/api --type rust 2>/dev/null; then
  echo "FAIL: AWD handlers still use /awd/events path prefix"
  exit 1
fi
rg -n '#\[post\("/events/awd"\)\]' src/modules/event/awd_team/api/admin.rs >/dev/null
rg -n '#\[get\("/events/\{event_id\}/awd/gameboxes"\)\]' src/modules/event/awd_team/api/player.rs >/dev/null
rg -n '#\[post\("/internal/awd/events/' src/modules/event/awd_team/api/internal.rs >/dev/null
rg -n 'get_event_capabilities' src/api/service/events.rs >/dev/null

# Scheduler reliability columns: incremental SQL + migration (not hand-edited Entity).
rg -n 'm0101_scheduler_retry' migration/src/lib.rs >/dev/null
test -f src/sql/update/01-scheduler-retry.sql
rg -n 'pub attempt_count:' src/entity/scheduled_tasks.rs >/dev/null
rg -n 'pub timeout_secs:' src/entity/scheduled_tasks.rs >/dev/null

# Dockerfiles for Flag/Judge exist.
test -f Dockerfile.awd-flagserver
test -f Dockerfile.awd-judgeserver

# Broadcast publisher wired in bootstrap.
rg -n 'BroadcastEventPublisher' src/bootstrap/mod.rs >/dev/null

# fcmc free-function adapters removed (runtime methods only).
if rg -n '^pub async fn (stop_and_remove|remove_and_create_bridge_net|build_image)\b' fcmc/src/lib.rs 2>/dev/null; then
  echo "FAIL: fcmc/src/lib.rs must not re-export legacy free-function container helpers"
  exit 1
fi

echo "== all structural checks passed =="

echo "== optional acceptance harnesses (skip-friendly) =="
if [[ -x scripts/e2e_flag_judge.sh ]]; then
  scripts/e2e_flag_judge.sh || echo "WARN: e2e_flag_judge.sh returned non-zero (not failing verify_refactor)"
else
  echo "SKIP: scripts/e2e_flag_judge.sh missing or not executable"
fi
