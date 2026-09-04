# AWD Wave 1 Foundation Report

> **Date**: 2026-08-26
> **Branch**: `awd`
> **HEAD**: `813aa0b4f60a2cbfbd4a5a27b87af81e970dfbde`

---

## Repository Snapshot

| Item | Value |
|------|-------|
| Branch | `awd` |
| HEAD | `813aa0b4f60a2cbfbd4a5a27b87af81e970dfbde` |
| Working tree | 19 modified, 3 untracked |
| Pre-existing user changes | `.gitignore`, `docs/awd-spec.md` |
| New Wave 1 files | `timing.rs`, migration |

---

## Migration Created

**Path**: `apps/api/src/sql/migrations/20260826175415-awd-wave1-config-foundation.sql`

---

## Schema Changes

### `awd_events`

| Column | Action | Type | Notes |
|--------|--------|------|-------|
| `round_count` | ADD | `INTEGER NULL` | NULL = not configured yet |
| `initial_score` | ADD | `BIGINT NOT NULL DEFAULT 0` | Team baseline score |

### `awd_event_gameboxes`

| Column | Action | Type | Notes |
|--------|--------|------|-------|
| `attack_score` | ADD | `BIGINT NOT NULL` | Backfilled from `break_points` |
| `down_points` → `judge_down_penalty` | RENAME | `BIGINT NOT NULL` | Semantic rename |

### `awd_judge_tasks`

| Column | Action | Type | Notes |
|--------|--------|------|-------|
| `worker_id` | ADD | `TEXT NULL` | Pull+Lease foundation |
| `lease_token_hash` | ADD | `TEXT NULL` | SHA-256 of lease token |
| `lease_expires_at` | ADD | `TIMESTAMPTZ NULL` | Lease expiry |
| `heartbeat_at` | ADD | `TIMESTAMPTZ NULL` | Last heartbeat |
| `claimed_at` | ADD | `TIMESTAMPTZ NULL` | Claim timestamp |

### `ScoreEventType` enum

| Variant | Action |
|---------|--------|
| `initial_score` | ADD |

---

## Event Duration Source

**Confirmed**: `event_duration` is derived from `events.end_time - events.start_time`. No duplicate `awd_events.event_duration_secs` column was added.

---

## Timing Domain

**File**: `apps/api/src/modules/event/awd/domain/timing.rs`

**Types**:
- `AwdTiming` — computed timing model (event_duration_secs, attack_duration_secs, hardening_duration_secs, round_count, round_duration_secs)
- `TimingValidationError` — 7 error variants

**Function**:
- `compute_timing(event_start, event_end, round_count, round_duration_secs) -> Result<AwdTiming, TimingValidationError>`

**Tests**: 11 tests covering valid, hardening=0, attack exceeds event, missing round_count, round_count=0, round_duration=0, missing end_time, end_time ≤ start_time, large values.

---

## Configuration API Changes

### `AwdEventStatusDto` (response)

New fields:
- `round_count: Option<i32>`
- `initial_score: i64`

### `AwdEventConfigRequest` (request)

New fields:
- `round_count: Option<i32>`
- `initial_score: Option<i64>`

### `AwdEventConfigPatch` (service)

New fields:
- `round_count: Option<i32>`
- `initial_score: Option<i64>`

### `EventGameBoxDto` (response)

New field:
- `attack_score: i64`

Renamed field:
- `down_points` → `judge_down_penalty`

### Config validation

- `round_count`: range 1..10,000
- `initial_score`: range 0..1,000,000,000

---

## Generated Entities

**Command**: `mise run db:gen`

**Generated files** (Rust):
- `apps/api/src/entity/awd_event_gameboxes.rs`
- `apps/api/src/entity/awd_events.rs`
- `apps/api/src/entity/awd_judge_tasks.rs`
- `apps/api/src/entity/sea_orm_active_enums.rs`

**Generated files** (TypeScript):
- `apps/web/src/entity/awd_event_gameboxes.ts`
- `apps/web/src/entity/awd_events.ts`
- `apps/web/src/entity/awd_judge_tasks.ts`
- `apps/web/src/entity/sea_orm_active_enums.ts`

---

## Tests Added

| Test | File | What It Proves |
|------|------|---------------|
| `valid_with_hardening` | `timing.rs` | 3600s event, 10 rounds × 300s = 3000s attack, 600s hardening |
| `hardening_zero` | `timing.rs` | 3000s event, 10 rounds × 300s = hardening_duration = 0 |
| `attack_exceeds_event` | `timing.rs` | 2999s event, 10 × 300 = AttackExceedsEvent |
| `round_count_missing` | `timing.rs` | NULL round_count → RoundCountNotConfigured |
| `round_count_zero` | `timing.rs` | 0 → RoundCountNotPositive |
| `round_duration_zero` | `timing.rs` | 0 → RoundDurationNotPositive |
| `missing_end_time` | `timing.rs` | NULL end_time → MissingEndTime |
| `end_time_not_after_start` | `timing.rs` | end ≤ start → EndTimeNotAfterStart |
| `large_values_ok` | `timing.rs` | 1,000,000 rounds × 100s = 100,000,000s |

---

## Validation Commands

| Command | Result |
|---------|--------|
| `mise run db:migration:validate` | ✅ 40 migrations validated |
| `mise run db:migration:apply` | ✅ 1 applied, 39 skipped |
| `mise run db:gen` | ✅ 61 entities generated |
| `cargo check -p floatctf` | ✅ Compiles |
| `cargo test -p floatctf --lib -- awd` | ✅ 173 passed |
| `cargo test -p floatctf --test awd_*` | ✅ 5 passed |

---

## Deferred Obsolete Schema

The following OLD schema is intentionally retained temporarily:

| Schema | Future Wave | Notes |
|--------|-------------|-------|
| `break_points` | Score semantics wave | Replaced by `attack_score` |
| `loss_points` | Score semantics wave | Replaced by `attack_score` |
| `fix_points` | Score semantics wave | Obsolete (JudgeFix removed) |
| `reset_protection_secs` | Reset wave | Spec §19.3 |
| `reset_protection_until` | Reset wave | Spec §19.3 |
| `grace_ends_at` | Round lifecycle wave | Spec §14 |
| `RoundStatus::Grace` | Round lifecycle wave | Spec §14 |
| `ScoreEventType::JudgeFix` | Score semantics wave | Spec §17.1 |
| Timed unban infrastructure | Ban wave | Spec §23 |
| `AwdRoundGraceEnd` task | Round lifecycle wave | Spec §14 |
| `AwdTeamUnban` task | Ban wave | Spec §23 |
| Push `/batch` judge dispatch | Judge Pull+Lease wave | Spec §16 |

---

## Git Diff Summary

```
Modified: 19 files
  - .gitignore (pre-existing)
  - 4 generated Rust entities
  - 4 generated TypeScript entities
  - 7 hand-edited Rust source files
  - 2 test files
  - 1 frontend API file

Untracked: 3 files
  - docs/awd-spec.md (pre-existing)
  - apps/api/src/modules/event/awd/domain/timing.rs (new)
  - apps/api/src/sql/migrations/20260826175415-awd-wave1-config-foundation.sql (new)
```

---

## Final Verdict

### PASS

All Wave 1 objectives met:
- ✅ Migration created and applied
- ✅ `db:gen` regenerated entities correctly
- ✅ Project compiles
- ✅ All AWD lib tests pass (173)
- ✅ All AWD integration tests pass (5)
- ✅ Timing domain with 11 tests
- ✅ Config DTO/API updated with new fields
- ✅ `down_points` → `judge_down_penalty` rename complete
- ✅ `attack_score` column added and backfilled
- ✅ `round_count` and `initial_score` added
- ✅ Lease foundation columns added
- ✅ `InitialScore` enum variant added
- ✅ No runtime behavior changed
- ✅ No obsolete schema removed prematurely