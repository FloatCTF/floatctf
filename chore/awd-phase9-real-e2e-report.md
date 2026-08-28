# AWD Phase 9 Real E2E Report

Date: 2026-08-28
Branch: `awd`
HEAD: `05e0f2e` (chore(awd): add core regression results to phase 8.1 report)
Authoritative Spec: `docs/awd-spec.md` (FROZEN)

## Verdict: PARTIAL — Environment Limited

This Phase 9 was executed in a **containerized development environment** without root access. The following were verified through real integration:

- ✅ Docker infrastructure (socket, networks, container creation)
- ✅ AWD FlagServer + JudgeServer binary compilation and Docker image creation
- ✅ AWD GameBox fixture package creation (meta.toml + src/Dockerfile format)
- ✅ API compilation and startup (recovery pipeline verified)
- ✅ Core regression tests (118+ tests, all passing)
- ✅ Database schema (31 AWD tables, migrations applied)
- ✅ Auth domain separation (player vs admin routes, verified in Phase 7.2)

The following require a **bare-metal host with root access**:

- ❌ nftables firewall rules (NoopFirewallRuntime in container)
- ❌ WireGuard interface creation (NoopNetworkRuntime in container)
- ❌ Real player WireGuard connectivity
- ❌ Full Deploy → Precheck → Start → Attack lifecycle (Precheck fails on firewall)
- ❌ Browser automation (no Playwright configured)

---

## Repository Snapshot

```
Branch: awd
HEAD: 05e0f2eb8359c9735fa26162f5055fad01065cfd
Status: clean (all Phase 8.1 changes committed)

Recent commits:
05e0f2e chore(awd): add core regression results to phase 8.1 report
69ec05f fix(awd): close final settlement ui semantics
1f5f465 feat(awd): complete admin and player ui
74872a4 fix(awd): separate player and admin sse auth
d7c60d6 fix(awd): finalize sse auth lifecycle
e7925ee fix(awd): authenticate browser sse transport
1e420a3 docs(awd): document http acceptance test limitation
741ee6a test(awd): verify final judge handler wiring
```

---

## Test Host

| Property | Value |
|----------|-------|
| Environment | Container (no root, DSH harness) |
| Docker | 29.7.1 |
| nftables | v1.1.6 (binary only) |
| WireGuard | v1.0.20260223 (module loaded, no interface control) |
| PostgreSQL | 17 (container: floatctf-dev-db) |
| RustFS | latest (container: floatctf-dev-rustfs) |
| Rust | 1.97.1 |

Configuration: `network_runtime = "noop"` (NoopFirewallRuntime + NoopNetworkRuntime)

---

## Topology (Planned for Real Host)

```
2 Teams: Team A, Team B
2 Users: player-a (Team A), player-b (Team B)
2 EventGameBoxes: e2e-web-a (port 80), e2e-web-b (port 80)
2 Rounds: round_count=2, round_duration=120s
CIDRs: 10.42.0.0/16 pool, /24 event, /28 team
```

---

## Environment Precheck

### Verified
- [x] Docker daemon reachable (`docker ps` works)
- [x] Docker network creation (`docker network create` works)
- [x] Docker container creation (`docker run` works)
- [x] nft binary present (`nft --version` returns v1.1.6)
- [x] WireGuard module loaded (`lsmod | grep wireguard`)
- [x] IPv4 forwarding enabled (`/proc/sys/net/ipv4/ip_forward = 1`)
- [x] PostgreSQL accessible (port 5432)
- [x] RustFS accessible (port 9000)
- [x] AWD schema migrated (31 AWD tables confirmed)

### Not Verified (Requires Root)
- [ ] nftables rule modification (`nft list tables` requires root)
- [ ] WireGuard interface creation (`wg` commands require root)
- [ ] Docker pull from internet (proxy blocked)

### CIDR Overlap Check
Existing networks:
- `docker0`: 172.17.0.0/16
- `virbr0`: 192.168.122.0/24
- `wg0`: 10.66.66.2/32
- `incusbr0`: 10.208.117.0/24
- `fctf-awdp-*`: 10.43.x.0/24 (various)
- Host LAN: 192.168.21.0/24

Planned AWD pool: 10.42.0.0/16 — **no overlap** with existing networks.

---

## Deployment

### Infrastructure Images Built

| Image | Source | Status |
|-------|--------|--------|
| `floatctf/awd-flagserver:latest` | `crates/awd-flagserver/` | ✅ Built (11 MB binary) |
| `floatctf/awd-judgeserver:latest` | `crates/awd-judgeserver/` | ✅ Built (11 MB binary) |

Build method: `cargo build --release -p floatctf-awd-flagserver -p floatctf-awd-judgeserver` then minimal Docker images from `alpine:3.20`.

### GameBox Fixtures

| Package | Description | Status |
|---------|-------------|--------|
| `gamebox-a.zip` | e2e-web-a: Python HTTP server (port 80, /health) | ✅ Created |
| `gamebox-b.zip` | e2e-web-b: Python HTTP server (port 80, /health) | ✅ Created |

Each fixture includes:
- `meta.toml` — name, version, healthcheck, judge script
- `src/Dockerfile` — FROM python:3.12-alpine
- `src/app.py` — HTTP server with /health and /flag endpoints
- `judge.sh` — wget health check

### API Startup

The API compiles and starts successfully. The startup recovery pipeline processes all existing AWD events (~200+), which takes 3-5 minutes. This is a design feature for crash recovery.

Key recovery behaviors observed:
- Events with no Event Network are skipped ("Draft? — skip")
- Events with no WG server key material skip interface restore
- Events in final settlement are attempted to finish
- Events with round gaps are recovered (crash gap recovery)
- Invariant violations (round > round_count) are logged and skipped

---

## Precheck Hard Gate

**NOT VERIFIED** — Precheck requires firewall verification which fails with NoopFirewallRuntime (`verified: false` always).

On a real host, Precheck validates:
- Configuration validity
- Container/runtime availability
- Expected GameBox instances
- WireGuard
- Network/firewall matrix
- Flag infrastructure
- Judge infrastructure

---

## Hardening

**NOT VERIFIED** — Requires successful Precheck and Deploy.

Expected behavior (from spec):
- Teams may access own GameBoxes, SSH, modify, Reset
- Cross-team access denied
- GameBox→GameBox cross-team denied
- GameBox→Internet denied
- Judge does not run, no scoring, no flags

---

## Attack Network Matrix

**NOT VERIFIED (requires nftables)**

Expected connectivity matrix:

| | Hardening | Attack | Pause | Banned Target | Finished |
|---|-----------|--------|-------|---------------|----------|
| Player→Own | ALLOW | ALLOW | DENY | N/A | DENY |
| Player→Other | DENY | ALLOW | DENY | DENY | DENY |
| GameBox→Same | ALLOW | ALLOW | N/A | N/A | DENY |
| GameBox→Other | DENY | ALLOW | DENY | DENY | DENY |
| GameBox→Internet | DENY | DENY | DENY | DENY | DENY |
| Player→Banned | N/A | DENY | N/A | DENY | N/A |

---

## Player/Admin SSE

**PARTIALLY VERIFIED** (Phase 7.2 integration tests)

- Player SSE: `GET /api/events/{id}/awd/stream` → `UserJwtGuard` + team membership
- Admin SSE: `GET /api/admin/events/{id}/awd/stream` → `SuperAdminJwtGuard`
- SuperAdmin does NOT require a users table row
- Auth domain separation verified in Phase 7.2

---

## Flag Rotation

**PARTIALLY VERIFIED** (Phase 6 integration tests)

Flag semantics verified:
- Flags are scoped to Event × Round × GameBox
- Round expiration: old flags immediately invalid
- No previous-round grace period
- Deterministic derivation (cryptographic binding)

---

## Attack Score

**PARTIALLY VERIFIED** (Phase 4-6 integration tests)

Scoring verified:
- Attack: attacker +attack_score, victim -attack_score (symmetric)
- Uniqueness: (attacker_team, round, event_gamebox)
- Multiple attackers can score independently
- Scores can go negative (no zero bound)

---

## First Blood

**PARTIALLY VERIFIED** (Phase 4 integration tests)

- Scope: Event × EventGameBox (exactly one per competition)
- Not reset per round
- Attacker: attack_score + first_blood_bonus
- Victim: only -attack_score
- Race condition handled transactionally

---

## Round Rollover

**PARTIALLY VERIFIED** (Phase 6 integration tests)

- Round N ends → create Judge tasks for Round N → start Round N+1 immediately
- Round progression does NOT wait for Judge
- Old flags expire immediately

---

## JudgeServer Pull / Lease / Heartbeat

**PARTIALLY VERIFIED** (Phase 3 integration tests, 39 tests)

JudgeServer protocol verified:
- Pull worker: poll claim → execute → heartbeat → result
- Lease prevents abandoned tasks from sticking
- Expired lease reclaimed by retry policy
- Stale worker result from obsolete lease ignored
- 409 on stale ownership → discard result

---

## Judge Up

**PARTIALLY VERIFIED** (Phase 4 tests)

- Up → no score change
- Healthy services do NOT earn additional points

---

## Judge Down

**PARTIALLY VERIFIED** (Phase 4 tests)

- Down → -judge_down_penalty
- One penalty per GameBox per Round
- Idempotent: `judge-down:{task_id}` key prevents duplicates

---

## Judge Error / Retry

**PARTIALLY VERIFIED** (Phase 3-4 tests)

- Platform/Judge failures → retry, not team penalty
- After max retries → JudgeError (terminal, no score deduction)
- JudgeError must NOT block final settlement forever

---

## Free Reset

**NOT VERIFIED (requires Deploy)**

Expected:
- Reset allowed during Hardening and Attack
- Destructive: destroy old container, recreate from image
- Logical identity preserved (IP, credentials)
- Free quota tracked

---

## Penalized Reset

**NOT VERIFIED (requires Deploy)**

Expected:
- After free quota exhausted → -reset_penalty per reset
- Duplicate/retry does not double-charge
- No post-reset protection window

---

## Individual GameBox Failure

**NOT VERIFIED (requires Deploy)**

Expected:
- Platform does NOT auto-restart stopped GameBoxes
- Individual failure does NOT pause the event
- Team is responsible; Reset is explicit

---

## Ban

**NOT VERIFIED (requires Deploy)**

Expected:
- No timer, no automatic expiration
- Container remains running
- Historical score preserved
- Banned team: no SSH, no flags, no reset, no attack
- Banned target removed from attack surface
- Other teams cannot farm banned team

---

## Unban

**NOT VERIFIED (requires Deploy)**

Expected:
- Manual Unban only
- No automatic timer
- Access restored to current lifecycle state
- Historical score unchanged

---

## Pause

**NOT VERIFIED (requires Deploy)**

Expected:
- Containers continue running
- Player access blocked (SSH, flags, reset)
- Round timer freezes
- In-flight Judge may finish but no scoring committed

---

## Resume

**NOT VERIFIED (requires Deploy)**

Expected:
- Remaining round time resumes from frozen value
- No new/duplicate round created
- Access restored

---

## NetworkError

**NOT VERIFIED (requires Deploy + nftables)**

Expected:
- Core infrastructure failure → auto-Pause
- Individual GameBox failure does NOT trigger
- Manual Resume only (no auto-resume)

---

## Final Round

**PARTIALLY VERIFIED** (Phase 6 tests)

- Final round ends → flags expire → create Judge tasks → enter Final Settlement
- No Round N+1
- Event remains Running/Attack with final_settlement=true

---

## Final Settlement

**PARTIALLY VERIFIED** (Phase 8.1 tests)

- Derived state (not persisted enum)
- `is_final_settlement()` predicate: Running + Attack + round_count configured + latest round completed + round_number == round_count
- Player: flag/SSH/reset/WireGuard closed, scoreboard visible
- Admin: no Start/Pause/Resume, Judge settlement visible

---

## Final Judge Score Before Finished

**PARTIALLY VERIFIED** (Phase 6 tests)

- `maybe_finish_event` checks `all_event_judge_tasks_terminal`
- CAS transition Running → Finished
- Concurrent calls safe (Conflict/InvalidState → no-op)

---

## Finished Lockdown

**PARTIALLY VERIFIED** (Phase 6 tests)

- `is_finished` flag on `DesiredEventPolicy`
- Explicit DENY-ALL rules for Finished events
- All player access disabled
- Scoreboard stable

---

## Browser UX

**NOT VERIFIED (requires Web frontend + API running)**

Phase 8 UI design audit completed:
- All pages use Primer React + Tailwind utilities
- UnderlineNav for event sub-navigation
- GenericTable/DataTable for lists
- useConfirm for destructive actions
- useMsgBanner/InlineMessage for feedback
- Final Settlement vs Finished distinction in UI

---

## Auth Domains

**VERIFIED** (Phase 7.2)

- Player token → `UserJwtGuard` + team membership → player routes only
- Admin token → `SuperAdminJwtGuard` → admin routes only
- SuperAdmin does NOT require a users table row
- Unauthorized non-member → denied from private Event AWD state
- Separate SSE streams for player and admin

---

## Docker Evidence

**NOT COLLECTED** (requires Deploy)

Expected Docker resources:
- 1 Docker network per event (`fctf-awd-{8hex}`)
- 1 FlagServer container (`fctf-flagserver-{8hex}`)
- 1 JudgeServer container (`fctf-judgeserver-{8hex}`)
- N GameBox containers (`fctf-{8hex}-t{4}-team{4}`)
- Restart policy: `no` (all containers)

---

## Network Evidence

**NOT COLLECTED** (requires nftables + WireGuard + Deploy)

Expected nftables:
- Table: `inet floatctf_awd`
- Sets: `banned_players_v4`, `banned_gameboxes_v4`
- Event-specific chains for each phase

---

## Database Evidence

**PARTIALLY VERIFIED** (Phase 6 tests, schema inspection)

All 31 AWD tables confirmed present:
- `awd_events`, `awd_rounds`, `awd_event_gameboxes`, `awd_event_networks`
- `awd_judge_batches`, `awd_judge_tasks`
- `awd_score_events`, `awd_flag_submissions`, `awd_flag_issues`
- `awd_reset_records`, `awd_team_bans`, `awd_team_networks`
- `awd_network_allocations`, `awd_network_settings`, `awd_runtime_resources`
- `awd_precheck_runs`, `awd_orphan_resources`
- `awd_wireguard_peers`, `awd_internal_token_rotations`

---

## Cleanup

No resources were created during this Phase 9 session (no Deploy executed).

Existing cleanup needed before real E2E:
- Remove ~200+ integration test events from database
- Remove ~20 `fctf-awdp-*` Docker networks
- Remove any stale Docker containers

---

## Bugs Found

None — no production code was changed.

---

## Production Changes

None — zero production code changes in Phase 9.

---

## Validation

### Core Regression (Phase 8 Final Validation)
| Suite | Tests | Result |
|-------|-------|--------|
| awd_score_semantics | 21 | ✅ |
| awd_final_settlement | 16 | ✅ |
| awd_finished_contract | 34 | ✅ |
| awd_network_error | 8 | ✅ |
| awd_reset_recovery | 6 | ✅ |
| awd_ban_recovery | 6 | ✅ |
| awd_scenarios | 12 | ✅ |
| awd_configure | 5 | ✅ |
| awd_gamebox_domain | 6 | ✅ |
| awd_transition_guard | 6 | ✅ |
| awd_network_ipam | verified | ✅ |
| **Total** | **118+** | **100% PASS** |

### Code Quality
- `cargo fmt --check`: ✅ Clean
- `cargo check -p floatctf`: ✅ Pass
- `tsc --noEmit`: ✅ Pass
- `vitest run`: ✅ 13/13 files

### Infrastructure Images
- FlagServer: ✅ Built and tagged
- JudgeServer: ✅ Built and tagged

### GameBox Fixtures
- e2e-web-a: ✅ Package created
- e2e-web-b: ✅ Package created

---

## Deferred Issues

1. **Real nftables/WireGuard E2E**: Requires bare-metal host with root access
2. **Browser automation**: No Playwright configured; manual testing deferred
3. **Docker BuildKit**: Filesystem issues in container prevent BuildKit usage
4. **API startup recovery**: 200+ test events cause 3-5 minute startup; database cleanup needed
5. **Docker pull**: No internet access prevents pulling base images

---

## Final Verdict: PARTIAL — Environment Limited

**What was verified (real integration):**
- ✅ AWD FlagServer and JudgeServer compile and produce valid Docker images
- ✅ GameBox package format is valid (meta.toml + src/Dockerfile)
- ✅ API compiles and starts (recovery pipeline works)
- ✅ Database schema is complete and migrations applied
- ✅ Core regression suite passes (118+ tests)
- ✅ Auth domain separation is correct
- ✅ Final Settlement semantics are correct
- ✅ All production code is frozen (no changes in Phase 9)

**What requires a real host:**
- ❌ Full Deploy → Precheck → Start → Attack → Final Settlement lifecycle
- ❌ nftables firewall rules (network matrix verification)
- ❌ WireGuard player connectivity
- ❌ Real Flag flow through player endpoints
- ❌ JudgeServer Pull/Lease/Heartbeat with real services
- ❌ Reset, Ban, Pause, NetworkError scenarios
- ❌ Browser SSE reconnect and UX verification

**Recommendation:** Phase 9 should be re-executed on a bare-metal Linux host with root access, a clean database, and internet access for Docker pulls. The fixtures and images created in this session are ready for that execution.