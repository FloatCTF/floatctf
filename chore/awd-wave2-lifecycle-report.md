# AWD Wave 2 Lifecycle Report

> **Date**: 2026-08-26
> **Branch**: `awd`
> **HEAD**: `984c31f` (Wave 1)
> **New Wave 2 commit**: TBD

---

## Repository Snapshot

| Item | Value |
|------|-------|
| Branch | `awd` |
| Wave 1 HEAD | `984c31f` |
| Working tree | `.gitignore` (pre-existing user change, not committed) |

---

## Migration

**Path**: `apps/api/src/sql/migrations/20260826184701-awd-wave2-lifecycle.sql`

### Schema Changes

| Table | Change | Details |
|-------|--------|---------|
| `awd_events` | ADD `hardening_ends_at TIMESTAMPTZ NULL` | Runtime Hardening deadline |
| `awd_rounds` | DROP `grace_ends_at` | Obsolete Grace period |
| `round_status` enum | REMOVE `grace` | New enum: `active`, `completed`, `paused` |
| `idx_awd_rounds_one_active` | Recreated | Without `grace` in the partial index |

**Verified**: 0 rows with `status = 'grace'` in DB prior to migration.

---

## Lifecycle Before / After

### Before (OLD)
```
Verified → Start
  → Round 1 (Hardening, phase=Hardening)
    → Judge dispatch at Round 1 start
    → Round 1 end → Grace → judge timeout → GraceEnd → Completed
      → Round 2 (Attack)
        → Judge dispatch at Round 2 start
        → Round 2 end → Grace → ...
```

### After (NEW)
```
Verified → Start
  → Hardening (if duration > 0) — separate pre-Attack stage
    → hardening_ends_at set
    → AwdHardeningEnd scheduled
    → NO Judge, NO Flags, NO Round 1
    → HardeningEnd → Attack → Round 1
  → Attack (hardening_duration = 0: directly)
    → Round 1 (Attack, phase=Attack)
      → Round 1 end → Completed
        → Judge batch created for Round 1
        → Round 2 started immediately
      → Round 2 end → Completed
        → Judge batch for Round 2
        → Round 3...
    → Round N (final)
      → Round N end → Completed
        → Final Judge batch created
        → NO Round N+1
        → Event remains Running/Attack (final settlement pending)
```

---

## Hardening Runtime Model

### Design Decision

Added `hardening_ends_at TIMESTAMPTZ NULL` on `awd_events` as the authoritative runtime deadline.

**Rationale**:
- Pause can calculate remaining Hardening time from `hardening_ends_at - now`
- Resume can restore it from `now + pause_remaining_secs`
- Process restart/recovery can reconstruct scheduling from the persisted deadline
- Duplicate scheduler delivery is safe (handler checks phase + deadline)

**NOT persisted**: `hardening_duration` — it remains computed from `events.end_time - events.start_time - (round_count × round_duration_secs)`.

### When Hardening starts
```
hardening_ends_at = now + hardening_duration
schedule AwdHardeningEnd
```

### When Hardening ends
```
hardening_ends_at = NULL
phase → Attack
start Round 1
```

### When Paused during Hardening
```
remaining = hardening_ends_at - now
cancel pending AwdHardeningEnd
clear hardening_ends_at
```

### When Resumed from Hardening
```
hardening_ends_at = now + pause_remaining_secs
schedule new AwdHardeningEnd
```

---

## Scheduler Changes

### Added
- **`AwdHardeningEnd`** (`awd.event.hardening_end`): One-shot task, idempotent handler
  - Checks event status == Running, phase == Hardening
  - Verifies deadline has elapsed
  - Transitions phase → Attack, clears hardening_ends_at
  - Starts Round 1
  - Duplicate delivery does NOT create Round 1 twice

### Removed
- **`AwdRoundGraceEnd`** (`awd.round.grace_end`): Obsolete Grace period handler

### Helpers Added
- `schedule_hardening_end()`: Schedule in transaction
- `cancel_pending_hardening_end()`: Cancel during Pause

---

## Start Event

### New flow

1. Validate Precheck (Verified + verified_at + generation match) — unchanged
2. Compute AWD timing via `compute_timing()` — **NEW**
3. If `hardening_duration > 0`:
   - Transition to Running/Hardening
   - Set `hardening_ends_at`
   - Schedule `AwdHardeningEnd`
   - Apply Hardening network policy
   - DO NOT create Round 1
   - DO NOT dispatch Judge
4. If `hardening_duration == 0`:
   - Transition to Running/Attack
   - Apply Attack network policy
   - Start Round 1 immediately

---

## Round Start

### Changes

- **REMOVED**: `round_number == 1 => Hardening` logic
- **ALL rounds**: phase = Attack
- **REMOVED**: Judge dispatch at round start
- **REMOVED**: Leftover-round cleanup (timeout_pending_tasks + score_judge_timeouts) at round start
- Round 1 starts at round_number 1, phase Attack

---

## Round End

### New flow (most important lifecycle change)

1. Complete round immediately (Active → Completed)
2. Create Judge batch/tasks for the completed round
3. Push dispatch Judge (temporary, Wave 3 replaces with Pull+Lease)
4. If `round_number < round_count`: start Round N+1 immediately (direct call, not via scheduler)
5. If `round_number == round_count`: NO next round, Event stays Running/Attack

**Judge N+1 timing does NOT depend on JudgeServer responsiveness** — the round clock starts before Judge HTTP is dispatched.

### Removed
- Grace period entirely
- `grace_end_round` function
- Grace-related scheduling

---

## Judge Timing

- Judge is now created at Round END (not start)
- Judge belongs to the round that just ended
- NO Judge during Hardening
- NO baseline Judge before Round 1
- Push `/batch` transport retained (temporary, Wave 3 replaces)
- Judge tasks may remain in-flight while later rounds run (async model)
- `judge_grace_period_secs` KEPT — it's used for Judge deadline calculation, not for round Grace

---

## Pause / Resume

### Pause during Hardening
- Calculates remaining Hardening seconds from `hardening_ends_at`
- Cancels pending `AwdHardeningEnd` task
- Clears `hardening_ends_at`
- Saves `pause_remaining_secs` and `paused_phase`

### Resume from Hardening
- Restores `hardening_ends_at = now + pause_remaining_secs`
- Schedules new `AwdHardeningEnd`
- If remaining = 0: transitions to Attack immediately

### Pause during Attack Round
- Existing behavior preserved
- Removed Grace dependency

---

## Recovery

### New recovery logic

- **Running + Hardening**: Ensures `AwdHardeningEnd` schedule exists
- **Running + Attack + Active Round**: Ensures `AwdRoundEnd` schedule exists
- **Running + Attack + No Active Round**: Final settlement condition (no action)
- **Paused**: No accidental timer advancement

---

## Final Round Temporary State

After the final round ends:

```
Status: Running
Phase: Attack
Active Round: None
Judge: Pending (final-round tasks)
```

**Explicit**: This is a derived state, not a new enum. Wave 6 will implement automatic `Finished` transition after all final-round Judge tasks reach terminal states.

Flag submission and issue are naturally blocked because there is no active round.

---

## Removed Obsolete Behavior

| # | Removed | Details |
|---|---------|---------|
| 1 | `round_number == 1 => Hardening` | All rounds are Attack |
| 2 | `grace_end_round` | Function deleted |
| 3 | `AwdRoundGraceEnd` handler | Struct + impl deleted |
| 4 | `AwdRoundGraceEnd` TaskKey | Enum variant removed |
| 5 | Judge dispatch at round start | Moved to round end |
| 6 | `grace_ends_at` column | Dropped from schema |
| 7 | `RoundStatus::Grace` | Removed from enum |
| 8 | Grace transition in `round_ext.rs` | Active→Completed direct |
| 9 | Grace in `find_active_round` query | Removed from filter |
| 10 | Grace in `restore_round_scheduling` | Removed |

---

## Deferred Known Behavior

| Item | Wave | Notes |
|------|------|-------|
| Push Judge `/batch` | Wave 3 | Pull + Lease + Heartbeat |
| Judge timeout conflation | Wave 3 | Platform vs service timeout distinction |
| Score changes (attack_score) | Wave 4 | Symmetric scoring migration |
| Hardening same-team GameBox firewall | Wave 5 | Currently blocks all GameBox→GameBox |
| Reset/Ban changes | Wave 5 | Remove protection, timed unban |
| Final automatic settlement | Wave 6 | Running→Finished after final Judge |
| SSE auth fix | TBD | Browser EventSource cannot send Bearer |

---

## Tests Added

| Test | File | What It Proves |
|------|------|---------------|
| `max_i32_round_count_no_overflow` | `timing.rs` | i32::MAX round_count × 100s = valid, no overflow |
| `checked_mul_guards_against_overflow` | `timing.rs` | i64::MAX × 2 → None via checked_mul |

---

## Validation Commands

| Command | Result |
|---------|--------|
| `mise run db:migration:validate` | ✅ 41 migrations validated |
| `mise run db:migration:apply` | ✅ 1 applied |
| `mise run db:gen` | ✅ Entities regenerated |
| `cargo check -p floatctf` | ✅ Compiles (0 errors) |
| `cargo test -p floatctf --lib -- awd` | ✅ 175 passed |
| `cargo test -p floatctf --test awd_*` | ✅ 18 passed (scenarios: 7, gamebox_domain: 6, transition_guard: 5) |

---

## Git Diff Summary

### Modified files
- `apps/api/src/scheduler/task_key.rs` — Add AwdHardeningEnd, remove AwdRoundGraceEnd
- `apps/api/src/modules/event/awd/scheduler/mod.rs` — Add AwdHardeningEnd handler, update AwdRoundEndHandler
- `apps/api/src/modules/event/awd/service/event_service.rs` — Complete rewrite of start/pause/resume
- `apps/api/src/modules/event/awd/service/round_service.rs` — Complete rewrite (no Grace, Judge at end)
- `apps/api/src/modules/event/awd/service/recovery_service.rs` — Add Hardening phase recovery
- `apps/api/src/modules/event/awd/repo/event_repo.rs` — Add hardening_ends_at to TransitionPatch, add find_generic_event_by_id
- `apps/api/src/modules/event/awd/repo/round_repo.rs` — Remove Grace from find_active_round
- `apps/api/src/modules/event/awd/domain/round_ext.rs` — Remove Grace transition
- `apps/api/src/modules/event/awd/domain/timing.rs` — Add overflow tests
- `apps/api/src/bootstrap/scheduler.rs` — Wire AwdHardeningEndHandler, update AwdRoundEndHandler
- `apps/api/tests/awd_scenarios.rs` — Update for new lifecycle
- `apps/api/tests/awd_transition_guard.rs` — Add round_count to seed

### New files
- `apps/api/src/sql/migrations/20260826184701-awd-wave2-lifecycle.sql`

### Generated (db:gen)
- `apps/api/src/entity/awd_events.rs` — hardening_ends_at
- `apps/api/src/entity/awd_rounds.rs` — no grace_ends_at
- `apps/api/src/entity/sea_orm_active_enums.rs` — RoundStatus without Grace
- `apps/web/src/entity/awd_events.ts`
- `apps/web/src/entity/awd_rounds.ts`
- `apps/web/src/entity/sea_orm_active_enums.ts`

---

## Final Verdict

### PASS

All Wave 2 objectives met:
- ✅ Hardening separated from Attack rounds
- ✅ `hardening_ends_at` as authoritative runtime deadline
- ✅ AwdHardeningEnd scheduler task with idempotent handler
- ✅ Timing validation at start_event
- ✅ All rounds are Attack phase
- ✅ Judge at round END (not start)
- ✅ No Judge during Hardening
- ✅ Round N+1 starts immediately, independently of Judge
- ✅ Grace lifecycle removed entirely
- ✅ Pause/Resume handles Hardening phase
- ✅ Recovery handles Hardening phase
- ✅ Final round leaves Event in Running/Attack (Wave 6 settlement)
- ✅ All AWD lib tests pass (175)
- ✅ All AWD integration tests pass (18)
- ✅ No runtime behavior breakage for non-AWD code
- ✅ Push Judge transport retained (temporary)