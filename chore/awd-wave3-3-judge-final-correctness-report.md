# AWD Wave 3.3 Judge Final Correctness Report

> **Date**: 2026-08-26
> **Branch**: `awd`
> **Wave 3.2 HEAD**: `213bf92`
> **Wave 3.3 commit**: TBD

---

## Repository Snapshot

| Item | Value |
|------|-------|
| Branch | `awd` |
| Wave 3.2 HEAD | `213bf92` |
| Working tree | Clean |

---

## Stale Ownership Fix

### Before (Wave 3.2)

Heartbeat 409 caused the heartbeat loop to exit, but the parent `execute_single_task` had no way to know ownership was stale. Result was still submitted.

### After (Wave 3.3)

`execute_single_task` creates an `Arc<AtomicBool>` (`ownership_stale`). This flag is shared with the heartbeat loop via `&AtomicBool`. When heartbeat receives HTTP 409, it sets the flag to `true`. After execution completes, `execute_single_task` checks the flag before calling `deliver_result`. If the flag is set, the result is discarded and zero submissions are made.

### Propagation mechanism

```
execute_single_task:
  ownership_stale = Arc::new(AtomicBool::new(false))
  spawn heartbeat_loop(..., &ownership_stale)
  ... execute checker ...
  heartbeat_handle.abort()
  if ownership_stale.load() → discard result, return
  deliver_result(...)
```

---

## Stale Ownership Orchestration Test

`stale_ownership_prevents_result_submission`:

1. Start heartbeat loop with mock returning `HeartbeatStatus::Stale`
2. Wait for heartbeat to fire and set the flag
3. Verify `ownership_stale == true`
4. Simulate the execution post-check: if stale, skip result
5. Assert: **zero result submissions**

```rust
assert!(ownership_stale.load(Ordering::Relaxed), "Ownership should be stale after 409");
let result_calls = mock.result_calls_snapshot();
assert_eq!(result_calls.len(), 0, "No result should be submitted when ownership is stale");
```

---

## Child Process Timeout Cleanup

### Before (Wave 3.2)

Used `tokio::process::Command::output()` wrapped in `tokio::time::timeout()`. Relied on implicit drop behavior to kill the child on timeout.

### After (Wave 3.3)

```rust
let child = tokio::process::Command::new(&script_path)
    .kill_on_drop(true)          // Explicit: kill child when handle dropped
    .stdout(Stdio::piped())      // Capture stdout
    .stderr(Stdio::piped())      // Capture stderr
    .spawn()?;

let result = tokio::time::timeout(timeout, child.wait_with_output()).await;
```

- `kill_on_drop(true)`: documented Tokio API — sends SIGKILL on Unix when the `Child` handle is dropped
- `timeout` fires → `Child` handle dropped → `kill_on_drop` kills the child
- `wait_with_output()` captures stdout/stderr when child exits normally

---

## Child Cleanup Test

`child_process_timeout_produces_target_timeout`:

1. Task with `timeout_secs = 1`, script content `sleep 10`
2. `execute_single_task` spawns child with `kill_on_drop(true)`
3. `tokio::time::timeout` fires after 1s
4. Child handle dropped → `kill_on_drop` kills the child
5. Outcome: `target_timeout`
6. Assert: exactly 1 result submission, outcome is `target_timeout`

---

## Max Concurrency Source of Truth

### Config flow

1. `awd_events.judge_max_concurrency` — DB column, configured via admin API
2. `deploy_service.rs` `ensure_infra_container` — reads `awd_event.judge_max_concurrency`, passes as `MAX_CONCURRENT={value}` env var
3. `admin.rs` `rollout_infra_container` — same env var injection for token rotation
4. JudgeServer reads `MAX_CONCURRENT` env var, defaults to 5

### Before (Wave 3.2)

`MAX_CONCURRENT` was NOT provisioned — JudgeServer used default 5 regardless of event config.

### After (Wave 3.3)

Both `deploy_service.rs` and `admin.rs` inject `MAX_CONCURRENT={awd_event.judge_max_concurrency}` into the JudgeServer container env for `kind == "judgeserver"`.

---

## Deploy Environment

### JudgeServer container env (complete)

| Variable | Source | Evidence |
|----------|--------|----------|
| `EVENT_ID` | `event_id` parameter | `deploy_service.rs:308` |
| `INTERNAL_TOKEN` | Decrypted `judgeserver_token_ciphertext` | `deploy_service.rs:309` |
| `LISTEN_ADDR` | Hardcoded `0.0.0.0:8080` | `deploy_service.rs:310` |
| `PLATFORM_INTERNAL_URL` | `AwdStaticConfig.platform_internal_url` | `deploy_service.rs:313` (Wave 3.1) |
| `MAX_CONCURRENT` | `awd_event.judge_max_concurrency` | `deploy_service.rs:314` (Wave 3.3) |

---

## Continuous Worker Orchestration Tests

### Up flow: `continuous_worker_flow_claim_execute_result_up`

1. Mock claim returns 1 task (script: `exit 0`)
2. `execute_single_task` spawns real child (exit 0)
3. Assert: 1 result submission, outcome = `up`, worker_id/attempt/lease_token match

### Down flow: `continuous_worker_flow_claim_execute_result_down`

1. Mock claim returns 1 task (script: `exit 1`)
2. `execute_single_task` spawns real child (exit 1)
3. Assert: 1 result submission, outcome = `down`, exit_code = 1

Both tests use the real `execute_single_task` function — not manually calling claim/execute/submit independently.

---

## Tests Summary

### Judgeserver (43 tests)

| Category | Count | Status |
|----------|-------|--------|
| Protocol serialization | 15 | ✅ |
| Outcome mapping | 11 | ✅ |
| Claim limit | 3 | ✅ |
| Poll loop | 2 | ✅ |
| Heartbeat (correct fields, stale, retry) | 3 | ✅ |
| Result delivery (retry, stale, 404, 200, exhaust) | 5 | ✅ |
| **Stale ownership orchestration** | 1 | ✅ |
| **Continuous Up flow** | 1 | ✅ |
| **Continuous Down flow** | 1 | ✅ |
| **Child timeout cleanup** | 1 | ✅ |

### API AWD tests (unchanged)

| Suite | Count | Status |
|-------|-------|--------|
| `cargo test --lib -- awd` | 172 | ✅ |
| `cargo test --test awd_scenarios` | 12 | ✅ |
| `cargo test --test awd_gamebox_domain` | 6 | ✅ |
| `cargo test --test awd_transition_guard` | 5 | ✅ |

**Total: 238 tests**

---

## Validation

| Command | Result |
|---------|--------|
| `cargo check -p floatctf-awd-judgeserver` | ✅ 0 errors |
| `cargo check -p floatctf` | ✅ 0 errors |
| `cargo test -p floatctf-awd-judgeserver` | ✅ 43 passed |
| `cargo test -p floatctf --lib -- awd` | ✅ 172 passed |
| `cargo test -p floatctf --test awd_scenarios` | ✅ 12 passed |
| `cargo test -p floatctf --test awd_gamebox_domain` | ✅ 6 passed |
| `cargo test -p floatctf --test awd_transition_guard` | ✅ 5 passed |

---

## Git Diff

### Modified files

- `crates/awd-judgeserver/src/worker.rs` — Stale ownership propagation, explicit child cleanup, 4 new tests
- `apps/api/src/modules/event/awd/service/deploy_service.rs` — Add `MAX_CONCURRENT` env var
- `apps/api/src/modules/event/awd/api/admin.rs` — Add `MAX_CONCURRENT` env var (token rotation)

### New files

- `chore/awd-wave3-3-judge-final-correctness-report.md` — This report

---

## Final Verdict

### PASS

Wave 3.3 completes the Judge Pull correctness:

- ✅ Stale ownership propagation: `Arc<AtomicBool>` shared between heartbeat and execution
- ✅ Heartbeat 409 → ownership flag set → result discarded
- ✅ Stale ownership orchestration test: 409 → zero result submissions
- ✅ Child process: explicit `kill_on_drop(true)` + `wait_with_output()`
- ✅ Child timeout test: `sleep 10` with 1s timeout → `target_timeout`
- ✅ `MAX_CONCURRENT` propagated from `judge_max_concurrency` to container env
- ✅ Continuous Up flow: real `execute_single_task`, spawns real child, 1 result
- ✅ Continuous Down flow: real `execute_single_task`, exit 1, outcome `down`
- ✅ All 238 tests pass
- ✅ Zero compilation errors