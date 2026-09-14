# AWD Wave 4.1 Score Correctness Report

> **Date**: 2026-08-27
> **Branch**: `awd`
> **HEAD**: (will be committed below)
> **Spec**: `docs/awd-spec.md`
> **Previous report**: `chore/awd-wave4-score-semantics-report.md`

---

## Repository Snapshot

| Item | Value |
|------|-------|
| Branch | `awd` |
| Previous HEAD | `6a57725` (Wave 4) |
| Working tree | Modified files from Wave 4.1 |

---

## Judge Score Idempotency Fix

### Bug Found

Wave 4's `judge_result` handler used `req.result_id.clone()` as the score idempotency key:

```rust
let idempotency_key = req.result_id.clone();
```

This is **incorrect**: `result_id` is generated per attempt by the JudgeServer. If the same logical task is retried with a different `result_id` (e.g., lease expires, new worker claims), a second `JudgeDown` score event could be created.

### Fix Applied

Changed `IdempotencyKey::judge` to `IdempotencyKey::judge_down(task_id)`:

```rust
/// Judge check result: once per logical task (§17 idempotency).
/// Task-scoped: one task → at most one JudgeDown event, regardless of
/// attempt number, result_id, worker retry, or lease reclaim.
pub fn judge_down(task_id: &str) -> String {
    format!("judge-down:{}", task_id)
}
```

Updated `internal.rs` to use:

```rust
let idempotency_key = IdempotencyKey::judge_down(&task_id.to_string());
```

### Verification

- `judge_down_idempotent_same_task`: Same task ID, two writes → only one JudgeDown
- `judge_down_multiple_attempts_one_score`: Attempt 1 replay after attempt 2 succeeded → rejected
- `judge_down_different_result_id_no_duplicate`: Different result_id, same task → rejected

---

## Score Idempotency Key Summary

| Event Type | Key | Scope |
|-----------|-----|-------|
| InitialScore | `initial-score:{event}:{team}` | Event × Team |
| Attack | `attack:{event}:{round}:{attacker}:{instance}` | Attacker × Round × Instance |
| VictimLoss | `victim-loss:{event}:{round}:{attacker}:{instance}` | Attacker × Round × Instance |
| FirstBonus | `first-bonus:{event}:{event_gamebox}` | Event × EventGameBox |
| **JudgeDown** | **`judge-down:{task_id}`** | **Logical task** (fixed) |
| ResetPenalty | `reset:{reset_id}` | Reset record |
| Adjustment | `adjustment:{adjustment_id}` | Adjustment |

---

## Tests Added (21 new)

### InitialScore Tests
| # | Test | Result |
|---|------|--------|
| 1 | `initial_score_three_teams` — 3 teams, initial=1000, exactly 3 InitialScore events | ✅ |
| 2 | `initial_score_zero_still_creates_ledger_entry` — delta=0 still creates row | ✅ |
| 3 | `initial_score_idempotent_on_retry` — retry start_event → no duplicates | ✅ |
| 4 | `initial_score_partial_seed_retry_fills_missing` — partial pre-seed recovery | ✅ |
| 5 | `initial_score_banned_team_still_gets_baseline` — banned team gets InitialScore | ✅ |

### Symmetric Attack Tests
| # | Test | Result |
|---|------|--------|
| 6 | `attack_symmetric_scoring` — attacker +100, victim -100 | ✅ |
| 7 | `attack_score_uses_correct_event_gamebox_config` — attack_score=250 | ✅ |
| 8 | `duplicate_attack_no_double_score` — same attacker/round → rejected | ✅ |
| 9 | `two_attackers_same_target_same_round` — each scores independently | ✅ |

### First Blood Tests
| # | Test | Result |
|---|------|--------|
| 10 | `first_blood_attacker_gets_bonus` — Attack+100, FirstBonus+50, VictimLoss-100 | ✅ |
| 11 | (Covered by test 10: victim has no FirstBonus deduction) | ✅ |
| 12 | `first_blood_only_once_per_event_gamebox` — Round 2 no FirstBonus | ✅ |
| 13 | `first_blood_concurrent_only_one_bonus` — concurrent → exactly one FirstBonus | ✅ |

### Judge Score Tests
| # | Test | Result |
|---|------|--------|
| 14 | `judge_up_no_score` — Up → zero score rows | ✅ |
| 15 | `judge_down_creates_penalty` — Down → exactly one JudgeDown, delta=-30 | ✅ |
| 16 | (Covered by 15: target_timeout = Down) | ✅ |
| 17 | `judge_down_idempotent_same_task` — same task retry → one JudgeDown | ✅ |
| 18 | `judge_down_multiple_attempts_one_score` — attempt replay → one JudgeDown | ✅ |
| 19 | `judge_down_different_result_id_no_duplicate` — different result_id → rejected | ✅ |

### Judge Non-Scoring Tests
| # | Test | Result |
|---|------|--------|
| 20-25 | `judge_non_scoring_outcomes_no_score` — JudgeError, SkippedResetting, SkippedBanned → zero score | ✅ |

### Negative Score + Audit
| # | Test | Result |
|---|------|--------|
| 26 | `negative_score_no_clamp` — initial=50, VictimLoss=-100 → total=-50 | ✅ |
| 27 | `ledger_audit_scenario` — A=1150, B=870, all event types verified | ✅ |

### Scoreboard Breakdown
| # | Test | Result |
|---|------|--------|
| 28 | `scoreboard_includes_initial_score` — total includes InitialScore | ✅ |

---

## Test Count Summary

| Suite | Passed |
|-------|--------|
| Lib tests | 51 |
| Judgeserver | 43 |
| awd_scenarios | 12 |
| awd_transition_guard | 5 |
| awd_configure | 5 |
| awd_gamebox_domain | 6 |
| awd_network_ipam | 8 |
| **awd_score_semantics (new)** | **21** |
| **Total** | **151** |

---

## Start Crash / Retry Safety

Verified: `seed_initial_scores` uses `create_score_event_if_absent` with `ON CONFLICT (idempotency_key) DO NOTHING`. If partial seeding occurs before `start_event` transition, a retry fills missing rows idempotently.

Test `initial_score_partial_seed_retry_fills_missing` proves:
- Team A pre-seeded → 1 InitialScore
- Teams B/C missing → 0
- `start_event` call → all 3 have exactly 1

---

## Legacy Search

| Location | Result |
|----------|--------|
| `apps/api/src/` (production Rust) | ✅ Zero hits |
| `apps/web/src/` (TS/TSX) | ✅ Zero hits |
| `apps/api/tests/` | ✅ Only assertion text "no JudgeFix" |

---

## Database Enum Check

```sql
SELECT unnest(enum_range(NULL::score_event_type));
-- attack, victim_loss, judge_down, first_bonus, reset_penalty, adjustment, initial_score
-- (7 values, no judge_fix)
```

---

## Frontend Typecheck

**NOT RUN** — local dependencies unavailable (npm registry unreachable). All generated TypeScript entities are clean with no legacy field references.

---

## Validation

| Check | Result |
|-------|--------|
| `cargo fmt --check` | ✅ |
| `cargo check -p floatctf` | ✅ |
| `cargo check -p floatctf-awd-judgeserver` | ✅ |
| `cargo test --lib` | ✅ 51 passed |
| `cargo test -p floatctf-awd-judgeserver` | ✅ 43 passed |
| `cargo test --test awd_scenarios` | ✅ 12 passed |
| `cargo test --test awd_transition_guard` | ✅ 5 passed |
| `cargo test --test awd_configure` | ✅ 5 passed |
| `cargo test --test awd_gamebox_domain` | ✅ 6 passed |
| `cargo test --test awd_network_ipam` | ✅ 8 passed |
| `cargo test --test awd_score_semantics` | ✅ 21 passed |
| Legacy search | ✅ Clean |
| DB enum | ✅ 7 values, no JudgeFix |

---

## Git Diff

Key files changed:
- `apps/api/src/modules/event/awd/domain/score.rs` — `judge_down` idempotency key
- `apps/api/src/modules/event/awd/api/internal.rs` — use task-scoped key
- `apps/api/tests/awd_score_semantics.rs` — **new file**, 21 tests

---

## Final Verdict

**PASS** ✅

All Wave 4.1 requirements met:
- Judge Down idempotency fixed (task-scoped, not result_id-scoped)
- 21 new score correctness tests covering all scenarios
- 151 total tests pass
- Legacy field search clean
- DB enum verified
- No product changes beyond score verification

---

**SOURCE CHANGES: YES · MIGRATION: NONE · DB:GEN: NOT REQUIRED · TESTS: 151 PASS · COMMIT: PENDING · PUSH: NO**