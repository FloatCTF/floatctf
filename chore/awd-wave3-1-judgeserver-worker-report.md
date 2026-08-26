# AWD Wave 3.1 JudgeServer Pull Worker Report

> **Date**: 2026-08-26
> **Branch**: `awd`
> **Wave 3 HEAD**: `71a404f`
> **Wave 3.1 commit**: TBD

---

## Repository Snapshot

| Item | Value |
|------|-------|
| Branch | `awd` |
| Wave 3 HEAD | `71a404f` |
| Working tree | Clean |

---

## Why Wave 3.1 Was Required

Wave 3 (`71a404f`) removed ALL Push dispatch code from the API side:
- `judge_service.rs`: removed `dispatch_batch`, all Push DTOs
- `round_service.rs`: removed `dispatch_judge_batch_for_round`, `score_judge_timeouts`
- `internal.rs`: removed `judge_callback` endpoint
- `scheduler/mod.rs`: removed `AwdRoundGraceEndHandler`

**But `crates/awd-judgeserver/src/main.rs` was never converted to the Pull worker.** The JudgeServer still had the old `POST /batch` receiver and `POST /judge/callback` sender. The system was incomplete — API could create Judge batches but nothing could execute them.

Wave 3.1 completes the Pull architecture end-to-end.

---

## Canonical Protocol

All routes confirmed from source (`apps/api/src/modules/event/awd/api/internal.rs`):

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/internal/awd/events/{event_id}/judge/claim` | Claim pending tasks |
| POST | `/internal/awd/events/{event_id}/judge/tasks/{task_id}/heartbeat` | Extend lease |
| POST | `/internal/awd/events/{event_id}/judge/tasks/{task_id}/result` | Submit execution result |
| GET | `/internal/awd/events/{event_id}/health` | Event health check |

All routes use `POST`. No GET `/judge/claim` — the Wave 3 report prose was incorrect; the source is authoritative.

---

## JudgeServer Architecture Before / After

### Before (Push)
```
API: Round End → create Judge batch → HTTP POST /batch to JudgeServer
JudgeServer: listen HTTP POST /batch → execute → callback POST /judge/callback
```

### After (Pull)
```
API: Round End → create Judge batch → schedule batch deadline (no HTTP)
JudgeServer: background poll loop → POST /judge/claim → execute → POST /judge/result
```

---

## JudgeServer Changes

### `crates/awd-judgeserver/src/main.rs`

| Change | Details |
|--------|---------|
| Removed | `POST /batch` endpoint, `JudgeBatch`, `JudgeTask`, `TaskResult`, `handle_batch`, `send_result` (old callback), callback retry logic |
| Added | `ClaimedTask`, `JudgeClaimRequest`, `JudgeClaimResponse`, `JudgeHeartbeatRequest`, `JudgeResultRequest` DTOs |
| Added | `poll_loop()` — background polling with backoff |
| Added | `claim_tasks()` — POST claim with `worker_id` + `limit` |
| Added | `heartbeat_loop()` — per-task heartbeat goroutine, exits on 409 stale |
| Added | `submit_result()` — POST result with stable `result_id` across retries, exponential backoff, stops on 409 stale |
| Added | `GET /ready` — readiness endpoint |
| Added | Graceful shutdown — `AtomicBool` + drain running tasks |
| Kept | `GET /health`, `build_script_env`, `truncate_str`, env whitelist, subprocess execution |
| Fixed | `result_id` generated once per attempt, reused across all result retries |

### New environment variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `PLATFORM_INTERNAL_URL` | `http://127.0.0.1:9090` | FloatCTF API base URL (container perspective) |
| `POLL_INTERVAL_SECS` | 5 | Sleep between polls when no work |
| `HEARTBEAT_INTERVAL_SECS` | 30 | Heartbeat frequency (must be < LEASE_TTL/3) |
| `LEASE_TTL_SECS` | 120 | For validation warning only |

### Worker ID

Generated at startup via `Uuid::new_v4()`. Does not survive restart. Not authentication — `INTERNAL_TOKEN` remains authentication.

---

## Deploy Integration

### `apps/api/src/core/config.rs`

Added `platform_internal_url: String` to `AwdStaticConfig` (previously only on `AwdpStaticConfig`).

### `apps/api/src/modules/event/awd/service/deploy_service.rs`

`ensure_infra_container()` now accepts `platform_internal_url: &str` parameter. For `kind == "judgeserver"`, adds `PLATFORM_INTERNAL_URL` to container env.

### `apps/api/src/modules/event/awd/api/admin.rs`

`rollout_infra_container()` now accepts `platform_internal_url: &str` parameter. Same conditional env injection for `kind == "judgeserver"`.

---

## Precheck Integration

**No changes needed.** The precheck (`precheck_service.rs`) only validates container health (existence + running), not the `/batch` endpoint. The judge_check note already says: "正式 judge 调用链 Phase 3 接入。precheck 不产生正式 Judge Task / score."

The precheck already uses `GET /health` via container-level health check, not the old `/batch` endpoint.

---

## Poll Loop

```
loop:
    if shutdown: break
    available = semaphore.available_permits()
    if available == 0: sleep(1s); continue
    claim(limit = available)
    if tasks empty: sleep(poll_interval); continue
    for each task: acquire permit → spawn execution
    sleep(100ms)  // yield for spawned tasks
```

### Over-claim prevention

`claim limit = available permits`, never pre-claims tasks that can't start immediately.

### Backoff

Transient API failures → log warning, sleep poll interval, retry. Never terminates permanently.

---

## Concurrency

Single authoritative mechanism: `Arc<Semaphore>`. Available permits determine claim limit. No separate poller semaphore or executor semaphore.

---

## Heartbeat

Per-task heartbeat loop:
- Waits first interval before starting
- POSTs heartbeat every `HEARTBEAT_INTERVAL_SECS`
- On 409 stale → exits loop (ownership lost)
- On transient failure → logs warning, retries
- Aborted when task execution completes

---

## Result Delivery

- Stable `result_id` generated once per task attempt
- Exponential backoff retry: 1s → 2s → 4s (max 4 attempts)
- 200 success → done
- 200 idempotent → done (backend handles dedup)
- 409 stale → stop retrying (ownership lost)
- 404 → stop retrying
- 5xx/network → retry with same `result_id`

---

## Outcome Mapping

| Worker Outcome | API Status | Retry? |
|---------------|------------|--------|
| `up` (exit 0) | Up | No |
| `down` (exit 1) | Down | No |
| `target_timeout` | Down (backend maps) | No |
| `worker_error` (spawn/script write failure) | JudgeError or Pending (backend decides) | Backend retries |
| Heartbeat 409 | Discard | No |

---

## Stale Ownership Handling

- Heartbeat 409 → heartbeat loop exits
- Result 409 → stops retrying immediately
- Worker crash → no fake completion; backend lease reclaim + deadline recovers

---

## Legacy Push Code Removed

| File | Removed |
|------|---------|
| `crates/awd-judgeserver/src/main.rs` | `POST /batch`, `handle_batch`, `JudgeBatch`, `JudgeTask`, `TaskResult`, old `send_result` (callback), callback retry, `has_valid_bearer_token`, `constant_time_eq` |

---

## Tests

### JudgeServer unit tests (4)

| Test | Status |
|------|--------|
| `script_env_allowlist_only_keeps_whitelisted_vars` | ✅ |
| `truncate_short_string_unchanged` | ✅ |
| `truncate_long_string_adds_truncation_note` | ✅ |
| `auth_header_has_bearer_prefix` | ✅ |

### API AWD tests (all pass)

| Suite | Count | Status |
|-------|-------|--------|
| `cargo test --lib -- awd` | 172 | ✅ |
| `cargo test --test awd_scenarios` | 12 | ✅ |
| `cargo test --test awd_gamebox_domain` | 6 | ✅ |
| `cargo test --test awd_transition_guard` | 5 | ✅ |

---

## Validation

| Command | Result |
|---------|--------|
| `cargo check -p floatctf-awd-judgeserver` | ✅ 0 errors |
| `cargo check -p floatctf` | ✅ 0 errors |
| `cargo test -p floatctf-awd-judgeserver` | ✅ 4 passed |
| `cargo test -p floatctf --lib -- awd` | ✅ 172 passed |
| `cargo test -p floatctf --test awd_scenarios` | ✅ 12 passed |
| `cargo test -p floatctf --test awd_gamebox_domain` | ✅ 6 passed |
| `cargo test -p floatctf --test awd_transition_guard` | ✅ 5 passed |

---

## Git Diff

### Modified files

- `crates/awd-judgeserver/src/main.rs` — Complete rewrite: Push → Pull worker
- `apps/api/src/core/config.rs` — Add `platform_internal_url` to `AwdStaticConfig` + `AwdToml`
- `apps/api/src/modules/event/awd/service/deploy_service.rs` — Pass `platform_internal_url` to JudgeServer container
- `apps/api/src/modules/event/awd/api/admin.rs` — Pass `platform_internal_url` to `rollout_infra_container`

### New files

- `chore/awd-wave3-1-judgeserver-worker-report.md` — This report

---

## Final Verdict

### PASS

Wave 3.1 completes the Pull Judge architecture:

- ✅ JudgeServer runs as background Pull worker
- ✅ Old `POST /batch` Push receiver deleted
- ✅ Old `POST /judge/callback` sender deleted
- ✅ Poll loop with proper backoff
- ✅ Concurrency controlled by single Semaphore
- ✅ Claim limit = available permits (no over-claim)
- ✅ Per-task heartbeat loop
- ✅ Stable result_id across retries
- ✅ Stale ownership (409) handled correctly
- ✅ Graceful shutdown with drain
- ✅ `PLATFORM_INTERNAL_URL` env var provisioned to container
- ✅ Precheck unaffected (already container-health based)
- ✅ All 199 AWD tests pass
- ✅ Zero new compilation errors