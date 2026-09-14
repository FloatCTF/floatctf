# AWD Phase 9 Environment Audit

Date: 2026-08-28
Branch: awd (05e0f2e)
Status: Environment audit for Phase 9 E2E validation

## Host Environment

| Property | Value |
|----------|-------|
| OS | Arch Linux (containerized) |
| Kernel | Linux (container) |
| Docker | 29.7.1 (API 1.51) |
| nftables | v1.1.6 (binary present, no root — cannot modify) |
| WireGuard | tools v1.0.20260223, module loaded |
| Rust | 1.97.1 (Arch) |
| Node | via mise |
| PostgreSQL | 17 (container: floatctf-dev-db) |
| RustFS | rustfs/rustfs:latest (container: floatctf-dev-rustfs) |

## Privileges

| Capability | Status | Impact |
|-----------|--------|--------|
| Root access | ❌ No sudo | Cannot modify nftables, WireGuard interfaces |
| Docker socket | ✅ Read/write | Can create/manage containers, networks |
| Internet access | ❌ Proxy blocked | Cannot pull Docker Hub images |
| nftables write | ❌ No root | `NoopFirewallRuntime` required |
| WireGuard create | ❌ No root | `NoopNetworkRuntime` required |
| IPv4 forwarding | ✅ Enabled | Docker networking works |

## Existing Infrastructure

### Running Containers
- `floatctf-dev-db` — PostgreSQL 17 (port 5432)
- `floatctf-dev-nginx` — Nginx (ports 7780-7781)
- `floatctf-registry` — Docker registry v2
- `floatctf-dev-rustfs` — RustFS S3-compatible storage (port 9000)

### Database
- AWD schema migrated (all 31 AWD tables present)
- ~200+ existing AWD events (integration test artifacts)
- 1 super_admin: `sysadmin` (00000000-0000-0000-0000-000000000000)
- Test users available (testuser, u-allok-*, u-allfail-*, etc.)

### Docker Networks
- `bridge`, `host`, `none` — default
- `fctf-awdp-*` — 20+ networks from AWDP integration tests
- `fctf-awdp-control`, `fctf-awdp-practice` — AWDP infrastructure
- `compose_default` — Docker Compose

### Available Docker Images
- `alpine:3.20`, `busybox:1.36`, `python:3.12-alpine`
- `rust:1.97.1-slim-bookworm` (build base)
- `floatctf/awd-flagserver:latest` — built in this session
- `floatctf/awd-judgeserver:latest` — built in this session
- `floatctf/infra/awdp-judgeserver:latest` — AWDP only
- Various gamebox images (test-g, test-gg, test-g2, test-c, e2e-web)

## Configuration

### AWD Config (`development.toml`)
```toml
[awd]
network_runtime = "noop"
flagserver_image = "floatctf/awd-flagserver:latest"
judgeserver_image = "floatctf/awd-judgeserver:latest"
```

### Key Implication
The `network_runtime = "noop"` setting means:
- `NoopFirewallRuntime` — nftables operations are no-ops (verify always returns false)
- `NoopNetworkRuntime` — WireGuard operations are no-ops
- Docker containers/networks ARE created via `fcmc::DockerRuntime`

## Built Artifacts

| Artifact | Path | Status |
|----------|------|--------|
| FlagServer binary | `target/release/awd_flagserver` (11 MB) | ✅ Built |
| JudgeServer binary | `target/release/awd_judgeserver` (11 MB) | ✅ Built |
| FlagServer image | `floatctf/awd-flagserver:latest` | ✅ Built |
| JudgeServer image | `floatctf/awd-judgeserver:latest` | ✅ Built |
| GameBox A fixture | `chore/awd-phase9-e2e/gamebox-a.zip` | ✅ Created |
| GameBox B fixture | `chore/awd-phase9-e2e/gamebox-b.zip` | ✅ Created |

## Known Constraints

### API Startup Time
The API startup recovery processes ALL existing AWD events (~200+). Each event takes ~1 second, resulting in 3-5 minute startup time. This is a design feature (crash recovery) but makes rapid E2E iteration impractical.

### Network Isolation
Without root access, nftables rules cannot be applied. The `NoopFirewallRuntime` always returns `verified: false`, which means Precheck will fail on the firewall verification step. Full network isolation testing requires a bare-metal or privileged host.

### WireGuard
Without root access, WireGuard interfaces cannot be created. The `NoopNetworkRuntime` makes all WG operations no-ops. Player WireGuard connectivity cannot be tested.

### Docker Pull
No internet access means Docker Hub images cannot be pulled. Only locally cached images are available. This was worked around by building images from local binaries.

## Recommendations for Real E2E

1. **Bare-metal host** with root access required for nftables + WireGuard
2. **Clean database** or separate test database to avoid recovery overhead
3. **Internet access** for pulling base images (or pre-cache all needed images)
4. **Docker BuildKit** enabled for efficient multi-stage builds
5. **Dedicated CIDR ranges** that don't overlap with host networks