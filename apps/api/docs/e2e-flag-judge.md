# FlagServer / JudgeServer E2E smoke

Environment-gated harness for the AWD infrastructure binaries.

## What it covers

| Mode | Trigger | Behaviour |
|------|---------|-----------|
| Default / CI-safe | `RUN_DOCKER_TESTS` unset | `cargo check` for `awd_flagserver` + `awd_judgeserver`, then skip Docker (exit 0) |
| Docker smoke | `RUN_DOCKER_TESTS=1` | Build `Dockerfile.awd-*`, run labeled containers, hit Judge `/health`, prove Flag listens, trap cleanup |

## Prerequisites

- Rust toolchain (always)
- Docker CLI + daemon (only when `RUN_DOCKER_TESTS=1`)
- `curl` (Docker smoke HTTP checks)
- Optional: `docker compose` if using the compose profile

## Run

From `src/floatctf-api` (or this repo root via the script path):

```bash
# Skip Docker (default) — safe in CI without privileges
./scripts/e2e_flag_judge.sh

# Full Docker smoke
RUN_DOCKER_TESTS=1 ./scripts/e2e_flag_judge.sh
```

Optional compose profile (network + both services):

```bash
RUN_DOCKER_TESTS=1 docker compose -f docker-compose.e2e.yml --profile e2e up --build -d
# ... manual checks ...
docker compose -f docker-compose.e2e.yml --profile e2e down -v --remove-orphans
```

Prefer `scripts/e2e_flag_judge.sh` for automated cleanup with unique `floatctf-e2e=<uuid>` labels.

## Safety rules

- Containers / images / networks are tagged with `floatctf-e2e=<uuid>`.
- Cleanup on `EXIT` trap only removes names/labels created by that run.
- Never stops or removes unlabeled or production containers.
- Does not modify host firewall or WireGuard.

## Smoke expectations

- **JudgeServer**: `GET /health` → `{"status":"ok"}` (container published on random host port).
- **FlagServer**: process stays running and accepts HTTP on `/flag` (platform may be absent; non-000 status is enough for smoke).
- Startup env uses dummy `EVENT_ID` / `INTERNAL_TOKEN` / unreachable `PLATFORM_INTERNAL_URL` — not a full issue-flag integration test.

## Related

- `Dockerfile.awd-flagserver`, `Dockerfile.awd-judgeserver`
- `src/bin/awd_flagserver.rs`, `src/bin/awd_judgeserver.rs`
- `scripts/verify_refactor.sh` (optional section for this harness)
