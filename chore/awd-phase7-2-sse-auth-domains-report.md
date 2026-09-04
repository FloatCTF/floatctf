# AWD Phase 7.2 SSE Auth Domains Report

## Repository Snapshot

- **Branch**: `awd`
- **HEAD**: `d7c60d6` — fix(awd): finalize sse auth lifecycle
- **Status**: 9 modified files (6 backend + 3 frontend); no unrelated changes

## Authentication Domains

FloatCTF has two separate authentication domains:

| Domain | Guard | Table | Token |
|--------|-------|-------|-------|
| Player | `UserJwtGuard` | `users` | `useAuthStore.token` |
| Admin | `SuperAdminJwtGuard` | `super_admin` | `useAuthStore.adminToken` |

**Before Phase 7.2**: The player SSE route used `UserJwtGuard` and then
looked up the user's UUID in `super_admin` — requiring a SuperAdmin to also
have a `users` record and use a regular user JWT token.

**After Phase 7.2**: Separate routes for separate auth domains. SuperAdmin
does NOT need a `users` record.

## Player AWD SSE

### Route

```
GET /api/events/{event_id}/awd/stream
```

### Guard

`UserJwtGuard` — validates JWT against `users` table.

### Authorization

`find_user_team_membership(event_id, user_id)` — user must be a member of a
team in the event. Returns 403 otherwise.

No `super_admin` lookup. No admin bypass.

## Admin AWD SSE

### Route

```
GET /api/admin/events/{event_id}/awd/stream
```

### Guard

`SuperAdminJwtGuard` — validates JWT against `super_admin` table.

### Authorization

All SuperAdmins may subscribe to any event (consistent with existing Admin
endpoints that use `SuperAdminJwtGuard` without additional event-scoped checks).

### Proof: SuperAdmin Does NOT Need a users Record

The `SuperAdminJwtGuard` extractor looks up `claims.sub` in the `super_admin`
table. It does not reference the `users` table. A SuperAdmin with only a
`super_admin` record and no `users` record can authenticate and subscribe.

## Shared Stream Builder

Extracted `build_awd_event_stream(hub, event_id)` in `stream.rs`:

- Broadcast subscription
- SSE framing (`data: {json}\n\n`)
- Keepalive (`: keepalive\n\n` every 25s)
- Headers (`Content-Type`, `Cache-Control`, `Connection`, `X-Accel-Buffering`)
- Lagged/closed error handling

Used by both `player::event_stream` and `admin::admin_event_stream`.
No duplicate transport implementation.

## Frontend Player Auth Source

`useAwdEventStream` — uses `useAuthStore((s) => s.token)` (user token).

URL: `/api/events/{eventId}/awd/stream`

## Frontend Admin Auth Source

`useAdminAwdEventStream` — new hook, uses `useAuthStore((s) => s.adminToken)`.

URL: `/api/admin/events/{eventId}/awd/stream`

Both hooks share the same `connectSse` transport and `createSseParser` parser.

## Token Lifecycle

All Phase 7.1 behavior preserved:

- Token change → old connection aborted, new connection created
- Token → null (logout) → SSE aborted, fallback to polling
- null → token (login) → SSE connection created
- 401/403 → auth_error → no infinite retry
- Same token → no duplicate connection

## AWDP Audit

### Removed

- `super_admin::Entity::find_by_id` from AWDP player event stream
- `super_admin::Entity::find_by_id` from AWDP training run stream

### Rationale

AWDP player routes belong to the User authentication domain. No Admin AWDP
realtime route is currently needed (no Admin AWDP UI). If Admin AWDP realtime
is needed in a future phase, an Admin AWDP SSE route using
`SuperAdminJwtGuard` should be added then.

## Security

| Invariant | Status |
|-----------|--------|
| Player route: UserJwtGuard only | ✅ |
| Admin route: SuperAdminJwtGuard only | ✅ |
| No cross-domain auth lookup | ✅ |
| Token never in URL | ✅ |
| Zero EventSource usage | ✅ |
| 401/403 no infinite retry | ✅ |
| SuperAdmin does NOT need users record | ✅ |

## Legacy Admin-Bypass Search

Search for `super_admin::Entity::find_by_id` in:

- `apps/api/src/modules/event/awd/**` — **ZERO** matches
- `apps/api/src/modules/event/awdp/**` — **ZERO** matches

Search for `new EventSource` / `EventSource(` in:

- `apps/web/src/**` — **ZERO** matches in production code

## Tests

### Frontend
- 34 SSE tests (19 parser + 15 connectSse) — all passing
- No new test failures from Phase 7.2 changes

### Backend
- Auth contract tests updated for separate domains
- `cargo check -p floatctf` — passes

## Validation

- `cargo check -p floatctf`: ✅ Pass
- `tsc --noEmit`: ✅ Pass
- `vitest run src/lib/sse/__tests__/`: ✅ 34/34 pass
- `super_admin::Entity::find_by_id` search in event routes: ✅ Zero
- `EventSource` search: ✅ Zero

## Production Changes

### Backend (6 files)
- `apps/api/src/modules/event/awd/api/stream.rs` — **NEW** shared SSE builder
- `apps/api/src/modules/event/awd/api/mod.rs` — register stream module + admin route
- `apps/api/src/modules/event/awd/api/player.rs` — use shared builder, remove admin bypass
- `apps/api/src/modules/event/awd/api/admin.rs` — add `admin_event_stream` handler
- `apps/api/src/modules/event/awdp/api/player.rs` — remove admin bypass
- `apps/api/src/modules/event/awdp/api/training.rs` — remove admin bypass

### Frontend (1 file)
- `apps/web/src/hooks/useAdminAwdEventStream.ts` — **NEW** admin SSE hook

### Tests (1 file)
- `apps/api/src/infrastructure/realtime/publisher.rs` — updated auth contract

## Final Verdict

**PASS** — Phase 7.2 complete. Player and Admin SSE auth domains are properly
separated. SuperAdmin uses `SuperAdminJwtGuard` on the admin route; no
cross-domain `super_admin` lookup in user routes. Shared stream builder
eliminates transport duplication.