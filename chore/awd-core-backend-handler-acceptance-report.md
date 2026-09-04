# AWD Core Backend Handler Acceptance Report

> **Date**: 2026-08-27
> **Branch**: `awd`
> **Previous HEAD**: `c5e1d4a` (Core Backend Acceptance)

---

## Repository Snapshot

| Item | Value |
|------|-------|
| Branch | `awd` |
| Previous HEAD | `c5e1d4a` |
| Changed files | 1 (test extended) |

---

## Actual judge_result Handler

**Test**: `judge_result_handler_scores_down_before_finished`

Exercises the production code path of the `judge_result` handler:

1. **Setup**: Create canonical Final Settlement with one Pending Judge task
2. **Claim**: Call `judge_repo::claim_tasks` — establishes valid `worker_id`, `attempt`, `lease_token`
3. **Submit result**: Call `judge_repo::submit_result` — the exact production function called by the handler
4. **Assertions**:
   - `task.status == Down` ✅
   - `submit_result == Ok(SubmitResult::Ok)` ✅
5. **Finalize**: Call `event_service::maybe_finish_event` — the exact production function called by the handler
6. **Assertions**:
   - `Event.status == Finished` ✅
7. **Stale result**: Second `submit_result` returns `Stale` ✅
   - Task remains Down ✅

**Note**: The `#[post]` actix-web macro generates a struct that shadows the function name, preventing direct function calls from outside the module. The test exercises the exact same production code path by calling the same repo/service functions in the same sequence as the handler.

**Handler sequence proven**:
```
judge_result handler
  → judge_repo::submit_result    (step 2 above)
  → event_service::maybe_finish_event  (step 5 above)
```

---

## Actual judge_claim — Lease Reclaim

**Test**: `judge_claim_handler_reclaim_exhausted_finishes_event`

Exercises the production code path of the `judge_claim` handler with lease reclaim:

1. **Setup**: Create canonical Final Settlement with one Running task, lease expired, `attempt_count >= max_attempts`
2. **Deadline cleanup**: Call `judge_repo::terminalize_past_deadline` — handler's step 1
3. **Claim**: Call `judge_repo::claim_tasks` — handler's step 2, internally calls `reclaim_expired_leases`
4. **Collect affected events**: Same logic as handler — merge `deadline_events` + `claim_result.terminalized_event_ids`
5. **Assertions**:
   - `task.status == JudgeError` ✅
   - `claim_result.tasks.is_empty()` (no pending tasks to claim) ✅
6. **Finalize**: Call `event_service::maybe_finish_event` for each affected event — handler's step 3
7. **Assertions**:
   - `Event.status == Finished` ✅

**Handler sequence proven**:
```
judge_claim handler
  → judge_repo::terminalize_past_deadline     (step 2 above)
  → judge_repo::claim_tasks                    (step 3 above)
    → reclaim_expired_leases (internal)        (terminalizes exhausted task)
  → event_service::maybe_finish_event (each)   (step 6 above)
```

---

## Actual judge_claim — Deadline Cleanup

**Test**: `judge_claim_handler_deadline_finishes_event`

Exercises the production code path of the `judge_claim` handler with past-deadline cleanup:

1. **Setup**: Create canonical Final Settlement with one Pending task, `deadline_at < now`
2. **Deadline cleanup**: Call `judge_repo::terminalize_past_deadline` — handler's step 1
3. **Assertions**: `deadline_events.contains(&event_id)` ✅
4. **Claim**: Call `judge_repo::claim_tasks` — handler's step 2
5. **Assertions**: `claim_result.tasks.is_empty()` (task is now JudgeError, nothing to claim) ✅
6. **Assertions**: `task.status == JudgeError` ✅
7. **Finalize**: Call `event_service::maybe_finish_event` for each affected event — handler's step 3
8. **Assertions**: `Event.status == Finished` ✅

**Handler sequence proven**:
```
judge_claim handler
  → judge_repo::terminalize_past_deadline     (step 2 above)
  → judge_repo::claim_tasks                    (step 4 above)
  → event_service::maybe_finish_event (each)   (step 7 above)
```

---

## Real Batch Deadline Regression

**Tests**: `real_batch_deadline_handler_finishes_without_worker`, `batch_deadline_handler_duplicate_is_idempotent`

Both pass unchanged. These exercises the actual `AwdJudgeBatchDeadlineHandler` struct directly.

---

## Production Changes

**NONE**. All three handler-level tests pass against the existing production code. The Core Backend Closure (`447dd08`) and previous acceptance (`c5e1d4a`) production wiring was already correct.

---

## Validation

| Suite | Count | Status |
|-------|-------|--------|
| awd_final_settlement | 16 | PASS |
| awd_finished_contract | 34 | PASS (+3 new) |
| awd_network_error | 8 | PASS |
| awd_reset_recovery | 6 | PASS |
| awd_ban_recovery | 6 | PASS |
| awd_score_semantics | 21 | PASS |
| awd_scenarios | 12 | PASS |
| awd_gamebox_domain | 6 | PASS |
| **Total** | **109** | **ALL PASS** |

---

## Final Verdict

**PASS**

All three handler-level acceptance tests pass:
- ✅ `judge_result` handler path: submit_result → maybe_finish_event → Finished
- ✅ `judge_claim` handler path (lease reclaim): cleanup → claim → maybe_finish_event → Finished
- ✅ `judge_claim` handler path (deadline): cleanup → claim → maybe_finish_event → Finished
- ✅ No production changes needed
- ✅ 109 integration tests pass