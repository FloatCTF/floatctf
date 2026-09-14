# AWD Wave 2.1 Lifecycle Correctness Report

> **Date**: 2026-08-26
> **Branch**: `awd`
> **Wave 2 HEAD**: `b8a6c8f`
> **Wave 2.1 commit**: TBD

---

## Repository Snapshot

| Item | Value |
|------|-------|
| Branch | `awd` |
| Wave 2 HEAD | `b8a6c8f` |
| Working tree | `.gitignore` (pre-existing user change) |

---

## Actual Round-End Execution Order Before Fix

### Source (round_service.rs `end_round` at Wave 2 commit `b8a6c8f`)

```
1. txn: Complete Round N → COMMIT
2. create_and_dispatch_judge_for_round(db, event_id, round_id).await
   └── create_batch()  [DB]
   └── dispatch_batch() [HTTP POST /batch to JudgeServer]
3. if is_final: return
   else: start_round(N+1).await
```

**Order**: Complete → Judge HTTP → Start N+1

**BUG**: The next round starts AFTER the JudgeServer HTTP call. If the JudgeServer is slow or unreachable, Round N+1 is delayed. The spec requires Round N+1 timing to be independent of JudgeServer responsiveness.

---

## Round-End Execution Order After Fix

### Current Source (Wave 2.1)

```
1. txn: Complete Round N → COMMIT
2. create_judge_batch_for_round()  [DB only, no HTTP]
3. if is_final: return
   else: start_round(N+1).await  [Round N+1 clock starts NOW]
4. dispatch_judge_batch_for_round()  [HTTP POST, last, best-effort]
```

**Order**: Complete → Judge batch (DB) → Start N+1 → Judge HTTP

**FIXED**: Round N+1 clock starts before the Judge HTTP call. Remote JudgeServer failure does not affect round timing.

### Split Functions

- `create_judge_batch_for_round()` — DB only, returns `batch_id`
- `dispatch_judge_batch_for_round()` — HTTP only, takes `batch_id`

---

## Crash Window Analysis

### Crash Window 1: Round N Completed → Round N+1 Start

```
Wave 2 code:
  Round N Completed (COMMIT)
  → [CRASH HERE]
  → Round N+1 never created

State after crash:
  Running + Attack + no active round
  Round N is Completed
```

### Crash Window 2: HardeningEnd → Attack → Round 1

```
Wave 2 code:
  HardeningEnd handler:
    phase → Attack, clear hardening_ends_at
    → [CRASH HERE]
    → Round 1 never created

State after crash:
  Running + Attack + no rounds
```

Both windows are now handled by the new `recover_round_gap` function.

---

## Recovery Truth Table

### Complete Recovery Logic

| Event State | Active Round? | Recovery Action |
|-------------|--------------|-----------------|
| Running + Hardening | N/A | Ensure HardeningEnd schedule exists |
| Running + Attack + Active Round N | Round N Active | Ensure RoundEnd(N) schedule exists |
| Running + Attack + No Active Round | No rounds exist | CASE A: Start Round 1 (HardeningEnd crash) |
| Running + Attack + No Active Round | Round N Completed, N < round_count | CASE B: Start Round N+1 (crash gap) |
| Running + Attack + No Active Round | Round N Completed, N == round_count | CASE C: No action (final settlement) |
| Running + Attack + No Active Round | Round N > round_count | CASE D: Log error, no action |
| Paused | Any | No timer advancement |

### New Function: `recover_round_gap`

Called from `restore_round_scheduling` when:
- `AwdEventStatus::Running`
- `AwdPhase::Attack`
- No Active/Paused round exists

Loads `round_count` from `awd_events` and `highest_round` from `awd_rounds` (ORDER BY round_number DESC), then applies the case logic above.

---

## HardeningEnd Crash Recovery

### Scenario

```
HardeningEnd handler:
  1. phase → Attack
  2. clear hardening_ends_at
  3. → [CRASH]
  4. start_round(1) never called
```

### Recovery

Recovery finds `Running + Attack + no rounds` → CASE A → idempotently starts Round 1.

**Test**: `hardening_end_crash_recovery_starts_round_1`

---

## Idempotency / Concurrency

### DB-level guarantees

- `awd_rounds` has `UNIQUE(event_id, round_number)` — prevents duplicate round numbers
- `awd_rounds` has partial unique index `idx_awd_rounds_one_active` on `(event_id, status) WHERE status IN ('active', 'paused')` — prevents duplicate active rounds
- `start_round` checks for existing round before creating → returns `created: false` on idempotent hit

### Service-level handling

- `restore_round_scheduling` returns 0 on second call (round already exists, idempotent)
- `recover_round_gap` delegates to `start_round` which is idempotent
- Recovery racing with scheduler: at most one Round N becomes Active (DB constraints)

**Test**: `crash_gap_recovery_idempotent`

---

## Tests Added

| # | Test | What It Proves |
|---|------|---------------|
| 1 | `crash_gap_no_rounds_recovers_round_1` | CASE A: Running/Attack + no rounds → Round 1 |
| 2 | `crash_gap_mid_round_recovers_next` | CASE B: Round 4 Completed, round_count=10 → Round 5 |
| 3 | `crash_gap_final_round_no_recovery` | CASE C: Round 5 Completed, round_count=5 → no Round 6 |
| 4 | `crash_gap_recovery_idempotent` | Double recovery → only 1 Round 4 |
| 5 | `hardening_end_crash_recovery_starts_round_1` | HardeningEnd crash → Round 1 |

---

## Validation

| Command | Result |
|---------|--------|
| `cargo check -p floatctf` | ✅ 0 errors |
| `cargo test -p floatctf --lib -- awd` | ✅ 175 passed |
| `cargo test -p floatctf --test awd_gamebox_domain` | ✅ 6 passed |
| `cargo test -p floatctf --test awd_scenarios` | ✅ 12 passed (7 original + 5 new) |
| `cargo test -p floatctf --test awd_transition_guard` | ✅ 5 passed |

**Total**: 198 AWD tests, 0 failures

---

## Git Diff

### Modified files
- `apps/api/src/modules/event/awd/service/round_service.rs` — Split Judge create/dispatch, add crash-gap recovery, update restore_round_scheduling signature
- `apps/api/src/modules/event/awd/service/event_service.rs` — Add publisher param to resume_event, update restore_round_scheduling call
- `apps/api/src/modules/event/awd/service/recovery_service.rs` — Add publisher param to recover_all/recover_event, update restore_round_scheduling call
- `apps/api/src/modules/event/awd/api/admin.rs` — Pass publisher to resume_event
- `apps/api/src/bootstrap/mod.rs` — Pass publisher to recover_all
- `apps/api/tests/awd_scenarios.rs` — Add 5 crash-gap tests, update imports, add seed_minimal_event_network helper

---

## Final Verdict

### PASS

All Wave 2.1 objectives met:
- ✅ Round-end ordering fixed: Round N+1 clock starts before Judge HTTP dispatch
- ✅ Crash-gap recovery handles all 4 cases (A/B/C/D)
- ✅ HardeningEnd crash recovery starts Round 1
- ✅ Recovery is idempotent (DB constraints + service-level checks)
- ✅ No new migration needed (existing schema + unique constraints sufficient)
- ✅ All 198 AWD tests pass (0 failures)
- ✅ No Wave 3 features implemented (Pull/Lease/Heartbeat untouched)