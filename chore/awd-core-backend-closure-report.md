# AWD Core Backend Closure Report

> **Date**: 2026-08-27
> **Branch**: `awd`
> **Previous HEAD**: `b447dbe` (Wave 6.2)

---

## Repository Snapshot

| Item | Value |
|------|-------|
| Branch | `awd` |
| Previous HEAD | `b447dbe` |
| Changed files | 3 (2 modified + 1 test extended) |

---

## Wave 6.2 Firewall Regression

**Verified**: All Wave 6.2 Finished firewall tests remain green:
- Finished in desired state ✅
- Finished Player→GameBox DENY ✅
- Finished GameBox→GameBox DENY ✅
- Finished GameBox→Internet/Host DENY ✅
- Finished recovery retains lockdown ✅

---

## Terminalization Trigger Contract

**Invariant**: Whenever a production operation changes one or more AWD Judge tasks from nonterminal to terminal, `maybe_finish_event(event_id)` is attempted promptly.

### Fix Applied

1. **`reclaim_expired_leases`**: Now returns `Vec<Uuid>` of event IDs where tasks were terminalized (JudgeError), not just a count.

2. **`terminalize_past_deadline`**: Now returns `Vec<Uuid>` of affected event IDs.

3. **`claim_tasks`**: Now returns `ClaimResult` struct containing both the claimed tasks and the terminalized event IDs from the reclaim step.

4. **`judge_claim` handler**: After claim completes, calls `maybe_finish_event` for each event that had tasks terminalized during cleanup.

### Trigger Paths Final Status

| Path | Terminal Status | Direct Finalizer? | Test |
|------|----------------|-------------------|------|
| Normal Up/Down | Up/Down | ✅ via `judge_result` | Wave 6.1 |
| `target_timeout` → Down | Down | ✅ via `judge_result` | Wave 6.1 |
| `worker_error` exhausted → JudgeError | JudgeError | ✅ via `judge_result` | Wave 6.1 |
| Lease reclaim exhausted → JudgeError | JudgeError | ✅ via `judge_claim` | `lease_reclaim_exhausted_last_task_finishes_event` |
| Claim-time deadline → JudgeError | JudgeError | ✅ via `judge_claim` | `claim_deadline_last_task_finishes_event` |
| Batch deadline → JudgeError | JudgeError | ✅ via `AwdJudgeBatchDeadlineHandler` | `real_batch_deadline_handler_finishes_without_worker` |
| SkippedResetting | SkippedResetting | N/A (sync, before Round creation) | Audited |
| SkippedBanned | SkippedBanned | N/A (sync, before Round creation) | Audited |

---

## Lease Reclaim → Finished

**Test**: `lease_reclaim_exhausted_last_task_finishes_event`

- Final Settlement with one Running task, lease expired, attempts exhausted
- `reclaim_expired_leases` returns `[event_id]`
- Task → JudgeError
- `maybe_finish_event` → Finished
- No JudgeDown score

---

## Claim Deadline → Finished

**Test**: `claim_deadline_last_task_finishes_event`

- Final Settlement with one Pending task, past deadline
- `terminalize_past_deadline` returns `[event_id]`
- Task → JudgeError
- `maybe_finish_event` → Finished
- No JudgeDown score

---

## Skipped Terminal Analysis

**`SkippedResetting`** and **`SkippedBanned`** are assigned during batch creation (before `end_round` creates the Judge batch), not asynchronously later. They are synchronous terminal states set at task creation time. Therefore they do not need a separate finalizer trigger — the `end_round` → `maybe_finish_event` call already covers them.

---

## Real Last-Down Result

**Verified in Wave 6.1**: The `judge_result` handler writes the score BEFORE calling `maybe_finish_event`. Score commit ordering is guaranteed by the existing architecture.

---

## Real Batch Deadline Handler

**Test**: `real_batch_deadline_handler_finishes_without_worker`

- Constructs the actual `AwdJudgeBatchDeadlineHandler` with production dependencies
- Creates a `scheduled_tasks::Model` with correct payload
- Calls `handler.run(task_model)` — the real production handler
- Task → JudgeError, Event → Finished
- No manual `terminalize_past_deadline` or `maybe_finish_event` calls

**Test**: `batch_deadline_handler_duplicate_is_idempotent`

- First invocation: Event → Finished
- Second invocation: no-op, Event remains Finished, task unchanged

---

## Event-Wide Judge Gate

**Test**: `older_round_terminalized_by_real_path_finishes_event`

- Round 5 Completed, Round 4 has Running task
- `maybe_finish_event` → NOT Finished (event-wide gate)
- `reclaim_expired_leases` → task → JudgeError
- `maybe_finish_event` → Finished

---

## Finished Score Freeze

**Test**: `score_freeze_after_real_down_finalization`

After Finished:
- Stale judge result → no-op
- Adjustment → rejected
- Duplicate finalizer → no-op
- Recovery → no-op
- Scoreboard unchanged

---

## Tests Added

| Test | What It Proves |
|------|---------------|
| `lease_reclaim_exhausted_last_task_finishes_event` | Lease reclaim → JudgeError → Finished |
| `claim_deadline_last_task_finishes_event` | Claim deadline → JudgeError → Finished |
| `real_batch_deadline_handler_finishes_without_worker` | Real handler → JudgeError → Finished |
| `batch_deadline_handler_duplicate_is_idempotent` | Handler idempotency |
| `older_round_terminalized_by_real_path_finishes_event` | Event-wide gate + real path |
| `score_freeze_after_real_down_finalization` | Score immutability |

**Total new tests**: 6

---

## Validation

| Suite | Count | Status |
|-------|-------|--------|
| awd_final_settlement | 16 | PASS |
| awd_finished_contract | 28 | PASS (+6 new) |
| awd_network_error | 8 | PASS |
| awd_reset_recovery | 6 | PASS |
| awd_ban_recovery | 6 | PASS |
| awd_score_semantics | 21 | PASS |
| awd_scenarios | 12 | PASS (1 pre-existing flaky) |
| awd_gamebox_domain | 6 | PASS |
| **Total** | **103** | **ALL PASS** |

---

## Production Fixes

1. **`reclaim_expired_leases`**: Returns `Vec<Uuid>` of terminalized event IDs
2. **`terminalize_past_deadline`**: Returns `Vec<Uuid>` of terminalized event IDs
3. **`claim_tasks`**: Returns `ClaimResult` with `terminalized_event_ids`
4. **`judge_claim` handler**: Calls `maybe_finish_event` for all affected events

---

## Git Diff

```
 apps/api/src/modules/event/awd/api/internal.rs     |  30 +++-
 apps/api/src/modules/event/awd/repo/judge_repo.rs   |  37 +++--
 apps/api/tests/awd_finished_contract.rs             | 535 ++++++++++++++++++
 3 files changed, 583 insertions(+), 19 deletions(-)
```

---

## Final Backend Verdict

**PASS**

All core backend closure requirements met:
- ✅ Finished firewall Wave 6.2 regression remains green
- ✅ Real batch deadline handler finishes Event
- ✅ Lease-reclaim exhaustion promptly finishes Event
- ✅ Claim-time deadline promptly finishes Event
- ✅ No reachable terminalization path can leave an all-terminal settlement waiting
- ✅ Event-wide judge gate composes correctly with real terminalization paths
- ✅ Score freeze after real finalization verified
- ✅ 103 integration tests pass (28 in contract suite)