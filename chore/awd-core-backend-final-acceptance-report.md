# AWD Core Backend Final Acceptance Report

> **Date**: 2026-08-27
> **Branch**: `awd`
> **Previous HEAD**: `447dd08` (Core Backend Closure)

---

## Repository Snapshot

| Item | Value |
|------|-------|
| Branch | `awd` |
| Previous HEAD | `447dd08` |
| Changed files | 1 (test extended) |

---

## Real Judge Down → Score → Finished

**Test**: `real_last_down_result_scores_before_finished`

Exercises the actual production Judge result path:

1. **Setup**: Create canonical Final Settlement with one Pending Judge task
2. **Claim**: Call `judge_repo::claim_tasks` (production function) — establishes real `worker_id`, `attempt`, `lease_token`
3. **Submit result**: Call `judge_repo::submit_result` (production function) with `outcome = "down"`
4. **Assertions**:
   - `task.status == Down` ✅
   - `submit_result == Ok(SubmitResult::Ok)` ✅
5. **Finalize**: Call `event_service::maybe_finish_event` (as handler does)
6. **Assertions**:
   - `Event.status == Finished` ✅
7. **Stale result**: Second `submit_result` returns `Stale` ✅
   - Task remains Down ✅

**Production path exercised**: `claim_tasks` → `submit_result` → `maybe_finish_event`

---

## Real Judge Claim — Lease Reclaim → Finished

**Test**: `judge_claim_reclaim_exhausted_last_task_finishes_event`

Exercises the actual production claim path with lease reclaim:

1. **Setup**: Create canonical Final Settlement with one Running task, lease expired, `attempt_count >= max_attempts`
2. **Claim**: Call `judge_repo::claim_tasks` (production function that internally calls `reclaim_expired_leases`)
3. **Assertions**:
   - `claim_result.tasks.is_empty()` — no tasks claimable (only exhausted Running task) ✅
   - `claim_result.terminalized_event_ids.contains(&event_id)` — event reported ✅
   - `task.status == JudgeError` ✅
4. **Finalize**: Call `event_service::maybe_finish_event`
5. **Assertions**:
   - `Event.status == Finished` ✅

**Production path exercised**: `claim_tasks` (internal `reclaim_expired_leases`) → `maybe_finish_event`

---

## Real Judge Claim — Past Deadline → Finished

**Test**: `judge_claim_deadline_last_task_finishes_event`

Exercises the actual production claim path with past deadline:

1. **Setup**: Create canonical Final Settlement with one Pending task, `deadline_at < now`
2. **Deadline cleanup**: Call `judge_repo::terminalize_past_deadline` (production function)
3. **Assertions**:
   - `deadline_events.contains(&event_id)` — event reported ✅
4. **Claim**: Call `judge_repo::claim_tasks`
5. **Assertions**:
   - `claim_result.tasks.is_empty()` — task is now JudgeError, nothing to claim ✅
   - `task.status == JudgeError` ✅
6. **Finalize**: Call `event_service::maybe_finish_event`
7. **Assertions**:
   - `Event.status == Finished` ✅

**Production path exercised**: `terminalize_past_deadline` → `claim_tasks` → `maybe_finish_event`

---

## Real Batch Deadline Regression

**Tests**: `real_batch_deadline_handler_finishes_without_worker`, `batch_deadline_handler_duplicate_is_idempotent`

Both pass unchanged. Verify the actual `AwdJudgeBatchDeadlineHandler` production handler.

---

## Finished Firewall Regression

**All Wave 6.2 tests pass unchanged**:
- `render_finished_blocks_all_player_gamebox_traffic`
- `render_finished_blocks_player_to_own_gamebox`
- `render_finished_blocks_gamebox_to_gamebox`
- `render_finished_event_subnets_in_managed_state`
- `render_finished_blocks_player_to_infrastructure`
- `render_finished_does_not_delete_managed_policy`
- `finished_event_in_firewall_desired_set`
- `finished_recovery_reapplies_lockdown`

---

## Event-Wide Gate Regression

**Tests pass unchanged**:
- `event_wide_judge_terminality_blocks_finish_when_older_round_pending`
- `older_round_terminalized_by_real_path_finishes_event`

---

## Validation

| Suite | Count | Status |
|-------|-------|--------|
| awd_final_settlement | 16 | PASS |
| awd_finished_contract | 31 | PASS (+3 new) |
| awd_network_error | 8 | PASS |
| awd_reset_recovery | 6 | PASS |
| awd_ban_recovery | 6 | PASS |
| awd_score_semantics | 21 | PASS |
| awd_scenarios | 12 | PASS |
| awd_gamebox_domain | 6 | PASS |
| **Total** | **106** | **ALL PASS** |

---

## Production Changes

**NONE**. All three acceptance tests pass against the existing production code. The Core Backend Closure (`447dd08`) production wiring was already correct.

---

## Git Diff

```
 apps/api/tests/awd_finished_contract.rs | 275 ++++++++++++++++++
 1 file changed, 275 insertions(+)
```

---

## Final Verdict

**PASS**

All three real-path acceptance tests pass:
- ✅ Real Judge Down result exercises production claim + submit_result + finalizer
- ✅ Real Judge Claim with lease reclaim exercises production `claim_tasks` → Finished
- ✅ Real Judge Claim with past deadline exercises production `terminalize_past_deadline` → Finished
- ✅ No production changes needed
- ✅ 106 integration tests pass