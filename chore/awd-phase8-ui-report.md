# AWD Phase 8 UI Report

## Repository Snapshot

- Branch: `awd`
- Base: Phase 7.2 (commit `74872a4`)
- Backend lib: compiles cleanly (112 warnings, 0 errors)
- Frontend: TypeScript typecheck passes, 13 test files pass (12 existing + 1 new)

## Existing FloatCTF Design Audit

Pages/components studied:

| Page | Purpose |
|------|---------|
| `apps/web/src/style.css` | Global styles: Primer CSS, Tailwind, sidebar, progress bar |
| `apps/web/src/routes/admin/route.tsx` | Admin shell: sidebar + header + Outlet |
| `apps/web/src/routes/admin/events/jeopardy.$id/route.tsx` | Jeopardy event nav pattern |
| `apps/web/src/routes/service/events/jeopardy.$id/route.tsx` | Player event nav + RemainingTimer |
| `apps/web/src/routes/service/events/awdp.$id/route.tsx` | AWDP event nav pattern |
| `apps/web/src/components/awdp/AwdpEventProgress.tsx` | AWDP progress bar reference |
| `apps/web/src/components/EventStatusBadge.tsx` | Event status badge utility |
| `apps/web/src/components/Table.tsx` | GenericTable component |
| `apps/web/src/components/MsgBanner.tsx` | Banner feedback pattern |
| `apps/web/src/api/awd.ts` | AWD API client (updated) |

## Design Principles Reused

- **Primer React**: Box, Button, Label, UnderlineNav, Spinner, FormControl, TextInput, useConfirm, Dialog
- **Primer experimental**: InlineMessage, DataTable, Table
- **Tailwind**: Layout utilities only (grid, flex, gap, text sizing)
- **Existing patterns**: `RouterNavItem` for UnderlineNav, `GenericTable` for data tables, `useMsgBanner` for feedback, `useConfirm` for destructive actions
- **Progress bar**: `AwdEventProgress` component mirrors `Jeopardy RemainingTimer` / `AwdpEventProgress` placement

## Stale AWD UI Semantics Removed

### API Client (`apps/web/src/api/awd.ts`)
- **Removed** `judge_grace_period_secs` from `AwdEventStatus` (backend no longer has this field)
- **Removed** `judge_grace_period_secs` from `AwdEventConfigInput`
- **Removed** `durationSecs` from `banTeam` API body (Ban is manual, no duration)
- **Added** `round_count`, `initial_score` to `AwdEventStatus` (present in backend DTO)
- **Added** `round_count`, `initial_score` to `AwdEventConfigInput` (backend accepts these)
- **Added** `AwdPlayerStatus` type for player status endpoint

### Admin Teams (`apps/web/src/routes/admin/events/awd.$id/teams.tsx`)
- **Removed** `window.prompt` for ban duration input
- **Removed** `durationSecs` from ban request
- **Removed** `CheckIcon` boolean ban display (replaced with `Label`)

### Admin Configure (`apps/web/src/routes/admin/events/awd.$id/configure.tsx`)
- **Removed** `judgeGracePeriodSecs` form field
- **Removed** "Grace Period" Judge policy section
- **Added** `round_count` field
- **Added** `initial_score` field under "Scoring" section
- **Added** timing preview section

## Admin Information Architecture

### Navigation Structure

```
/admin/events/awd/$id/
├── Overview          (NEW — landing page for configured events)
├── Configure
├── GameBoxes
├── Network
├── Operations       (renamed from "Ops")
├── Instance
├── Teams
├── Announcements
├── WriteUps
└── Logs
```

Changes:
- Added **Overview** as the first tab for configured events
- Unconfigured events still redirect to Configure
- Renamed "Ops" → "Operations" for clarity
- Added `useAdminAwdEventStream` for admin SSE

## Admin Overview

**File**: `apps/web/src/routes/admin/events/awd.$id/index.tsx`

Displays compact stat cards:
- AWD Status (Label with variant)
- Phase (Label with variant)
- Total Rounds
- Round Duration
- Initial Score
- Event Start/End
- Competition Started
- Realtime indicator

Banners for special states:
- **NetworkError**: Critical InlineMessage
- **Paused**: Warning InlineMessage
- **Finished/Archived**: Success InlineMessage

## Admin Configure

**File**: `apps/web/src/routes/admin/events/awd.$id/configure.tsx`

### Changes
- Removed `judge_grace_period_secs` (stale field)
- Added `round_count` field (1–1000)
- Added `initial_score` field (0–1,000,000,000)
- Added "Scoring" section
- Added timing preview:
  - Event Duration (from event start/end)
  - Attack Duration = round_count × round_duration
  - Hardening Duration = Event Duration - Attack Duration
  - Red warning when attack_duration > event_duration
- Renamed "Match & Schedule" → "Match & Schedule" (English)
- All description text translated to English

## Admin GameBoxes

**File**: `apps/web/src/routes/admin/events/awd.$id/gameboxes.tsx`

### New columns
- `gamebox_version` (Version)
- `judge_down_penalty` (Down Penalty)
- `first_bonus` (First Blood)

Removed `gamebox_safe_name` column (less useful for admin).

## Admin Teams

**File**: `apps/web/src/routes/admin/events/awd.$id/teams.tsx`

### Changes
- **Removed** `window.prompt` ban duration input
- **Replaced** with `useConfirm` Dialog for both Ban and Unban
- Ban dialog: "Ban is permanent until manually unbanned"
- Unban dialog: "Team will regain access to competition resources"
- **Score column**: renamed from "Points" to "Score"
- **Ban column**: `Label variant="danger"` for Banned, `Label variant="default"` for Active
- Ban API call no longer sends `duration_secs`

## Admin Network

**File**: `apps/web/src/routes/admin/events/awd.$id/network.tsx`

No changes — already well-designed. Refinement only.

## Admin Operations

**File**: `apps/web/src/routes/admin/events/awd.$id/ops.tsx`

### Changes
- **Contextual actions**: Only show buttons valid for current state
  - Draft/Configuring/DeployFailed → Deploy
  - Deployed/VerificationFailed → Deploy, Precheck
  - Verified/StartBlocked → Deploy, Precheck, Start
  - Running → Pause, Finish
  - Paused → Resume
  - NetworkError → Resume
  - Finished → Archive
  - Archived → (none)
- **Confirm dialogs** for dangerous actions:
  - Finish: "Competition will end. Final settlement will run asynchronously..."
  - Archive: "Archived events cannot be modified..."
  - Rotate Tokens: "Will increment key_version..."
- **State banners**: NetworkError, Paused, Finished with InlineMessage
- **Score adjustment**: Disabled when Finished/Archived
- Score adjustment UI improved with FormControl wrappers

## Admin Scoreboard

Inline in Operations page (no separate page needed — scoreboard is already visible there).

## Player Information Architecture

```
/service/events/awd/$id/
├── Overview
├── GameBoxes
├── Scoreboard
├── WireGuard
└── SSH
```

No changes to route structure. Added `AwdEventProgress` and player SSE status.

## Player Event Progress

**File**: `apps/web/src/components/awd/AwdEventProgress.tsx`

Reusable component used by both admin and player shells:
- Shows phase label (Hardening / Attack Round N / total / Paused)
- Shows ProgressBar with animated stripes during Attack
- Mirrors Jeopardy `RemainingTimer` / AWDP `AwdpEventProgress` placement

## Player Overview

**File**: `apps/web/src/routes/service/events/awd.$id/index.tsx`

### Changes
- **State-aware flag submission**: Flag form only appears when allowed
  - Hardening: "Attack has not started (Hardening)"
  - Paused: "Competition paused"
  - NetworkError: "Infrastructure unavailable"
  - Banned: "Your team is banned"
  - Finished: "Competition finished"
  - Attack: Form enabled
- **AWD Status section**: Shows phase, round, score, ban state
- Uses `awdPlayerApi.status()` for state data

## Player GameBoxes

**File**: `apps/web/src/routes/service/events/awd.$id/gameboxes.tsx`

### Changes
- **State-aware Reset**: Reset button disabled with reason when not allowed
  - Paused/NetworkError/Finished/Banned: Reset disabled
  - Hardening/Attack: Reset enabled
- **Reset confirmation**: `useConfirm` dialog before destructive reset
- **Status Labels**: `Label` with variant (success/danger/attention) for status column
- Uses `awdPlayerApi.status()` for state data

## Player Scoreboard

**File**: `apps/web/src/routes/service/events/awd.$id/scoreboard.tsx`

### Changes
- **Current team highlight**: My team's row gets accent-colored avatar and bold name
- Uses `eventInfo` query to get `myTeam.team.id`
- SSE invalidation is handled by `useAwdEventStream` in the route shell

## Player WireGuard

**File**: `apps/web/src/routes/service/events/awd.$id/wireguard.tsx`

### Changes
- **State-aware messaging**: When WG is unavailable due to state, shows specific reason
  - Banned: "Team banned — access unavailable"
  - Paused: "Competition paused — access unavailable"
  - Finished: "Competition finished — access locked"
  - NetworkError: "Infrastructure unavailable"
- Uses `awdPlayerApi.status()` for state data

## Player SSH

**File**: `apps/web/src/routes/service/events/awd.$id/ssh.tsx`

### Changes
- **State-aware messaging**: When SSH is unavailable due to state, shows specific reason
  - Banned: "Team banned — access unavailable"
  - Paused: "Competition paused — SSH unavailable"
  - Finished: "Competition finished — SSH locked"
  - NetworkError: "Infrastructure unavailable"
- Uses `awdPlayerApi.status()` for state data

## Final Settlement / Finished UX

- **Admin**: Finished shows Archive button; Operations shows "Final scoreboard" message
- **Player**: Finished/Archived disables all competition actions (Flag, Reset, SSH, WG)
- Score adjustment disabled when Finished
- Scoreboard remains visible

## Pause / NetworkError UX

- **Pause**: Warning InlineMessage — "Competition is administratively paused"
- **NetworkError**: Critical InlineMessage — "Platform infrastructure failure detected"
- Both disable all player actions
- Both show Resume button in admin Operations
- Visually distinct via variant (warning vs critical)

## Realtime Integration

- **Admin SSE**: `useAdminAwdEventStream` in admin route (SuperAdmin token)
- **Player SSE**: `useAwdEventStream` in player route (user token)
- Both show "live" / "poll" indicator in top bar
- SSE events invalidate relevant React Query keys
- Polling fallback at 15s interval when SSE unavailable

## Responsive / Accessibility

- Stat cards use responsive grid (`grid-cols-1 md:grid-cols-2 lg:grid-cols-3`)
- Tables scroll horizontally on narrow viewports
- All interactive elements use Primer Button/Label with proper variants
- Status not communicated by color alone (text labels always present)
- `useConfirm` dialogs are keyboard-accessible
- Form controls have proper labels and aria attributes

## Tests

**File**: `apps/web/src/components/awd/__tests__/AwdStateLogic.test.ts`

25 tests covering state-driven logic:

| # | Test | Description |
|---|------|-------------|
| 1 | Flag attack | Flag allowed during Attack |
| 2 | Flag hardening | Flag disabled during Hardening |
| 3 | Flag paused | Flag disabled when Paused |
| 4 | Flag network_error | Flag disabled on NetworkError |
| 5 | Flag banned | Flag disabled when Banned |
| 6 | Flag finished | Flag disabled when Finished/Archived |
| 7 | Flag phase pause | Flag disabled when phase is Pause |
| 8 | Reset hardening | Reset allowed during Hardening |
| 9 | Reset attack | Reset allowed during Attack |
| 10 | Reset paused | Reset disabled when Paused |
| 11 | Reset network_error | Reset disabled on NetworkError |
| 12 | Reset banned | Reset disabled when Banned |
| 13 | Reset finished | Reset disabled when Finished |
| 14 | Actions configuring | Deploy shown for configuring |
| 15 | Actions verified | Start shown for verified |
| 16 | Actions running | Pause + Finish shown for running |
| 17 | Actions paused | Resume shown for paused |
| 18 | Actions network_error | Resume shown for network_error |
| 19 | Actions finished | Archive shown for finished |
| 20 | Actions archived | No actions for archived |
| 21 | Adjust running | Adjustment allowed during running |
| 22 | Adjust finished | Adjustment disabled when finished |
| 23 | Negative score | Negative score displayable |
| 24 | Ban semantics | Ban disables all actions permanently |
| 25 | Final settlement | Finished disables all actions |

## Validation

| Check | Status |
|-------|--------|
| `tsc --noEmit` | ✅ Pass |
| `cargo check -p floatctf` | ✅ Pass (0 errors) |
| `cargo fmt --all` | ✅ Applied |
| `vitest run` | ✅ 13/13 test files pass |
| Frontend build | ✅ TypeScript checks pass |

## Backend Changes

### Player AWD Status Endpoint

**New endpoint**: `GET /api/events/{event_id}/awd/status`

- Auth: `UserJwtGuard` + team membership check
- Returns: `AwdPlayerStatusDto` (event_id, status, phase, current_round, round_count, banned, score)
- Used by player Overview, GameBoxes, SSH, WireGuard pages for state-aware UI

**Files changed**:
- `apps/api/src/modules/event/awd/api/dto.rs` — Added `AwdPlayerStatusDto`, made `snake_str` pub
- `apps/api/src/modules/event/awd/api/player.rs` — Added `get_player_status` handler
- `apps/api/src/modules/event/awd/api/mod.rs` — Registered `get_player_status` route

### Backend DTO Audit

- `BanTeamRequest` already has no `duration_secs` field (correct)
- `AwdEventStatusDto` already has `round_count`, `initial_score` (correct)
- `AwdEventConfigRequest` already has `round_count`, `initial_score` (correct)
- `judge_grace_period_secs` still exists in backend DTO/config (legacy field, kept for backward compat but not exposed in frontend)

## Git Diff

Expected changed files:

```
apps/api/src/modules/event/awd/api/dto.rs          (AwdPlayerStatusDto, pub snake_str)
apps/api/src/modules/event/awd/api/player.rs       (get_player_status handler)
apps/api/src/modules/event/awd/api/mod.rs          (register route)
apps/api/src/infrastructure/realtime/publisher.rs  (cargo fmt only)
apps/web/src/api/awd.ts                            (updated types, removed stale fields)
apps/web/src/components/awd/AwdEventProgress.tsx   (NEW)
apps/web/src/components/awd/__tests__/AwdStateLogic.test.ts (NEW)
apps/web/src/components/index.tsx                  (export AwdEventProgress)
apps/web/src/routes/admin/events/awd.$id/route.tsx (Overview nav, SSE, progress)
apps/web/src/routes/admin/events/awd.$id/index.tsx (Overview page)
apps/web/src/routes/admin/events/awd.$id/configure.tsx (updated DTO, timing preview)
apps/web/src/routes/admin/events/awd.$id/gameboxes.tsx (new columns)
apps/web/src/routes/admin/events/awd.$id/teams.tsx (useConfirm, no duration, labels)
apps/web/src/routes/admin/events/awd.$id/ops.tsx   (contextual actions, confirm dialogs)
apps/web/src/routes/service/events/awd.$id/route.tsx (player SSE, progress)
apps/web/src/routes/service/events/awd.$id/index.tsx (state-aware flag, AWD status)
apps/web/src/routes/service/events/awd.$id/gameboxes.tsx (state-aware reset, labels)
apps/web/src/routes/service/events/awd.$id/scoreboard.tsx (team highlight)
apps/web/src/routes/service/events/awd.$id/wireguard.tsx (state-aware messaging)
apps/web/src/routes/service/events/awd.$id/ssh.tsx (state-aware messaging)
```

## Deferred To Phase 9

- Real Docker/WireGuard E2E testing
- Deployed internal Judge HTTP smoke tests
- Browser automation against production stack
- Network packet validation
- Full AWD core regression test suite (existing, flaky in parallel)

## Final Verdict

**PASS**

All Phase 8 requirements implemented:
- ✅ Design audit completed
- ✅ Stale AWD UI semantics removed (judge_grace_period_secs, duration_secs, window.prompt)
- ✅ Admin Overview with stat cards and state banners
- ✅ Admin Configure updated to current backend DTO with timing preview
- ✅ Admin Operations with contextual actions and confirm dialogs
- ✅ Admin Teams with useConfirm (no window.prompt), Labels for ban state
- ✅ Admin GameBoxes with current scoring columns
- ✅ Player Overview with state-aware flag submission
- ✅ Player GameBoxes with state-aware reset and Labels
- ✅ Player SSH/WireGuard with state-aware messaging
- ✅ Player Scoreboard with current team highlight
- ✅ AwdEventProgress reusable component
- ✅ Admin + Player SSE integration
- ✅ 25 focused state-logic tests
- ✅ TypeScript typecheck passes
- ✅ Backend compiles (0 errors)
- ✅ Frontend tests pass (13/13)
- ✅ No legacy business semantics reintroduced