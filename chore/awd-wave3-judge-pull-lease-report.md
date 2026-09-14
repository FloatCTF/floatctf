# AWD Wave 3 Judge Pull + Lease Report

> **Date**: 2026-08-26
> **Branch**: `awd`
> **Wave 2.1 HEAD**: `ebf0494`
> **Wave 3 commit**: TBD

---

## Repository Snapshot

| Item | Value |
|------|-------|
| Branch | `awd` |
| Wave 2.1 HEAD | `ebf0494` |
| Working tree | `.gitignore` (pre-existing) |

---

## Architecture Before / After

### Before (Push)
```
API: Round End → create Judge batch → HTTP POST /batch to JudgeServer
JudgeServer: receive batch → execute → callback POST /judge/callback
```

### After (Pull)
```
API: Round End → create Judge batch → schedule batch deadline
JudgeServer: poll GET /judge/claim → claim tasks → heartbeat → POST /judge/result
API: validate lease → record result → score if applicable
```

---

## Schema / Migration

**Migration**: `20260826200153-awd-wave3-judge-pull-lease.sql`

- Removed `judge_timeout` from `judge_task_status` enum
- Existing `judge_timeout` rows converted to `judge_error` (0 rows existed)
- Updated `judge_grace_period_secs` comment: "Judge work deadline budget"

**No new columns added** — Wave 1 lease columns (`worker_id`, `lease_token_hash`, `lease_expires_at`, `heartbeat_at`, `claimed_at`) are now used.

---

## Judge Task State Machine

```
                     ┌─────────────┐
                     │   Pending   │
                     └──────┬──────┘
                            │ claim (worker claims task)
                     ┌──────▼──────┐
              ┌──────│   Running   │──────┐
              │      └──────┬──────┘      │
              │             │             │
     lease expires    submit result  absolute deadline
     + retries left        │             │
              │      ┌──────▼──────┐      │
              │      │ Up / Down   │      │
              │      └─────────────┘      │
              │                           │
              │                    ┌──────▼──────┐
              └────────────────────│ JudgeError  │
                                   └─────────────┘
                                   (NO score penalty)
```

### Terminal states
- `Up` — service healthy, JudgeFix score (temporary, Wave 4)
- `Down` — service unhealthy, JudgeDown penalty
- `JudgeError` — platform/Judge failure, NO score
- `SkippedResetting` — GameBox resetting, NO score
- `SkippedBanned` — team banned, NO score

### Removed
- `JudgeTimeout` — conflated platform failure with service failure

---

## Lease Protocol

### Token generation
- 32 random bytes → hex string
- Stored as SHA-256 hash only
- Plaintext returned once to worker on claim

### TTL
- `LEASE_TTL_SECS = 120` (2 minutes)
- `HEARTBEAT_INTERVAL_SECS = 30` (must be < TTL/3)

### Heartbeat
- Extends `lease_expires_at` to `min(now + TTL, deadline_at)`
- Validates: status=Running, worker_id matches, attempt matches, token hash matches

### Reclaim
- Running tasks with `lease_expires_at < now`
- If `attempt_count < max_attempts` AND `now < deadline_at`: back to Pending
- Otherwise: JudgeError

---

## Absolute Deadline

### Two distinct concepts

| Field | Meaning |
|-------|---------|
| `lease_expires_at` | Current execution ownership deadline |
| `deadline_at` | Absolute task completion deadline across ALL attempts |

### No-worker scenario
- `AwdJudgeBatchDeadline` scheduler task fires at `deadline_at`
- Terminalizes all non-terminal tasks in batch as JudgeError
- `terminalize_past_deadline()` runs on every claim to clean up

### Scoring
- Deadline expiry → JudgeError → NO score penalty
- Target service timeout → Down → JudgeDown penalty (via `target_timeout` → `Down` mapping)

---

## Internal APIs

### POST /internal/awd/events/{event_id}/judge/claim
- Auth: JudgeServer internal token
- Request: `{ "worker_id": "...", "limit": 5 }`
- Response: `{ "tasks": [{ task_id, batch_id, ..., lease_token, script_content, target_ip, ... }] }`
- Max 20 tasks per claim

### POST /internal/awd/events/{event_id}/judge/tasks/{task_id}/heartbeat
- Auth: JudgeServer internal token
- Request: `{ "worker_id": "...", "attempt": 1, "lease_token": "..." }`
- 200 OK / 409 Stale / 404 Not Found

### POST /internal/awd/events/{event_id}/judge/tasks/{task_id}/result
- Auth: JudgeServer internal token
- Request: `{ "worker_id": "...", "attempt": 1, "lease_token": "...", "result_id": "...", "outcome": "up|down|target_timeout|worker_error", "exit_code": 0, ... }`
- 200 OK / 200 Idempotent / 409 Stale / 404 Not Found

### Removed
- `POST /internal/awd/events/{event_id}/judge/callback` (old callback)

---

## Outcome Mapping

| Worker Outcome | Task Status | Retry? | Score? |
|---------------|-------------|--------|--------|
| `up` | Up | No | JudgeFix (temporary) |
| `down` | Down | No | JudgeDown |
| `target_timeout` | Down | No | JudgeDown |
| `worker_error` (attempts < max, before deadline) | Pending (released) | Yes | No |
| `worker_error` (exhausted) | JudgeError | No | No |
| Lease expiry (retries left) | Pending | Yes | No |
| Lease expiry (exhausted) | JudgeError | No | No |
| Absolute deadline | JudgeError | No | No |

---

## Round-End Judge Flow

```
end_round:
  1. Complete Round N → COMMIT
  2. create_judge_batch_for_round() → DB only (creates batch + tasks)
  3. schedule_batch_deadline_task() → AwdJudgeBatchDeadline
  4. If non-final: start_round(N+1)
  5. Publish SSE
  6. DONE — NO HTTP dispatch
```

---

## Removed Push Code

| File | Removed |
|------|---------|
| `judge_service.rs` | `dispatch_batch`, `JudgeDispatchTask`, `JudgeDispatchBatch`, `JudgeDispatchResponse`, `JudgeBatchStatus`, `serialize_script_args`, `judge_batch_endpoint`, `limit_error_body`, `set_batch_status`, `pending_task_count` |
| `round_service.rs` | `dispatch_judge_batch_for_round`, `score_judge_timeouts` |
| `judge_repo.rs` | `timeout_pending_tasks` |
| `internal.rs` | `judge_callback`, `is_duplicate_key` |
| `api/mod.rs` | `judge_callback` route registration |

---

## Removed Timeout Conflation

| Old | New |
|-----|-----|
| `JudgeTimeout` enum | REMOVED |
| `timeout_pending_tasks` (Pending+Running → JudgeTimeout) | REMOVED |
| `score_judge_timeouts` (JudgeTimeout → JudgeDown score) | REMOVED |
| Platform failure → JudgeDown penalty | FIXED: Platform failure → JudgeError (no penalty) |
| Target service timeout → JudgeTimeout | FIXED: Target timeout → Down (competition penalty) |

---

## Validation

| Command | Result |
|---------|--------|
| `db:migration:validate` | ✅ 42 migrations |
| `db:migration:apply` | ✅ Applied |
| `db:gen` | ✅ Entities regenerated |
| `cargo check -p floatctf` | ✅ 0 errors |
| `cargo test -p floatctf --lib -- awd` | ✅ 172 passed |
| `cargo test -p floatctf --test awd_scenarios` | ✅ 12 passed |
| `cargo test -p floatctf --test awd_gamebox_domain` | ✅ 6 passed |
| `cargo test -p floatctf --test awd_transition_guard` | ✅ 5 passed |

---

## Deferred To Wave 4+

| Item | Wave |
|------|------|
| JudgeServer rewrite (pull worker) | Wave 3.1 |
| InitialScore runtime credit | Wave 4 |
| Symmetric attack_score runtime | Wave 4 |
| Remove JudgeFix positive scoring | Wave 4 |
| Remove break/loss/fix legacy columns | Wave 4 |
| Reset protection removal | Wave 5 |
| Ban redesign | Wave 5 |
| Hardening same-team network fix | Wave 5 |
| Final automatic settlement | Wave 6 |

---

## Git Diff

### Modified files
- `apps/api/src/sql/migrations/20260826200153-awd-wave3-judge-pull-lease.sql` (new)
- `apps/api/src/scheduler/task_key.rs` — Add AwdJudgeBatchDeadline
- `apps/api/src/modules/event/awd/repo/judge_repo.rs` — Complete rewrite (lease, claim, heartbeat, result, deadline)
- `apps/api/src/modules/event/awd/service/judge_service.rs` — Remove all Push code, keep create_batch
- `apps/api/src/modules/event/awd/service/round_service.rs` — Remove Push dispatch, add batch deadline scheduling
- `apps/api/src/modules/event/awd/api/internal.rs` — Replace callback with claim/heartbeat/result
- `apps/api/src/modules/event/awd/api/dto.rs` — Add Wave 3 DTOs
- `apps/api/src/modules/event/awd/api/mod.rs` — Update route registration
- `apps/api/src/modules/event/awd/scheduler/mod.rs` — Add AwdJudgeBatchDeadline handler + restore
- `apps/api/src/modules/event/awd/service/recovery_service.rs` — Add batch deadline recovery
- `apps/api/src/modules/event/awd/domain/round_ext.rs` — Remove JudgeTimeout from is_terminal
- `apps/api/src/entity/sea_orm_active_enums.rs` — Generated (no JudgeTimeout)
- `apps/web/src/entity/sea_orm_active_enums.ts` — Generated (no JudgeTimeout)

---

## Final Verdict

### PASS

All Wave 3 objectives met:
- ✅ API never pushes Judge batches (Push dispatch removed)
- ✅ JudgeServer can poll Claim endpoint
- ✅ Lease with token hash (SHA-256, never stored plaintext)
- ✅ Heartbeat extends lease
- ✅ Expired leases reclaimable
- ✅ Attempt fencing (task_id + attempt + lease_token)
- ✅ Stale results rejected
- ✅ Service failure → Down (with JudgeDown penalty)
- ✅ Platform failure → JudgeError (no penalty)
- ✅ Worker error retries (back to Pending)
- ✅ Absolute deadline terminalizes uncompleted tasks
- ✅ Batch deadline scheduler guarantees terminalization
- ✅ JudgeTimeout enum removed
- ✅ Timeout conflation eliminated
- ✅ Round progression independent of Judge
- ✅ All 195 AWD tests pass