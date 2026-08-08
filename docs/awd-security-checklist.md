# AWD 安全清单（可执行化，P5-12）

> 将 V2 §22 的安全要求转为**可 grep / 可单测断言**，CI-fast 层自动执行。
> 每个条目给出断言方式；违规 = CI 失败。

| # | 要求 | 断言方式 | 状态 |
|---|------|---------|------|
| 1 | internal API 全部带服务身份认证 | `apps/api/tests/internal_auth_contract.rs::every_internal_route_requires_internal_auth`（源码扫描 3 个端点首参 `AwdInternalAuth`）+ live 401 断言 | ✅ 已实现 |
| 2 | crypto 无零密钥回退（fail-fast） | `grep -rn "vec!\[0u8; 32\]" apps/api/src/bootstrap` 零命中；`BootstrapError::Crypto` 路径存在 | ✅ Phase 0 |
| 3 | secrets 不明文落库（AEAD） | 单测 `decrypt_event_secret_matches_create_path_aad` / `test_wrong_aad_rejected`；落库路径全 `encrypt()` | ✅ |
| 4 | Flag 不明文落库（只存 hash） | `flag_repo` 只写 SHA-256（`hash_flag`）；grep 无明文 flag 写入路径 | ✅ |
| 5 | Judge stdout/stderr 截断 | `crates/awd-judgeserver` 4096 截断（`truncate_str` 单测） | ✅ |
| 6 | 限流存在 | `RateScope::{Submit,Reset,Internal}` + submit/reset/internal 端点接线（P5-10） | ✅ |
| 7 | path traversal 防护（target_ip 走 argv） | `system/command.rs` 仅 structured argv；grep 无 `sh -c` 拼接 | ✅ |
| 8 | Noop 不能 Verified | precheck 单测：Noop firewall 结构必 fail（`check_firewall_structure`） | ✅ |
| 9 | IPv6 阻断 | 渲染器单测 `render_ipv6_explicit_policy_present`（`ip6 saddr ... drop` 显式存在）；E2E IPv6 bypass（Phase 5） | ✅ |
| 10 | GameBox 无公网 | 渲染器单测：Hardening/Attack 均含 `@{k}_gameboxes_v4 drop`；E2E probe | ✅ |
| 11 | 管理员敏感操作审计 | `AuditAction::{TokenRotated,TeamBanned,TeamUnbanned,GameboxReset,ScoreAdjusted}` 有调用点（P5-11） | ✅ |
| 12 | shell 命令用 argv | `grep -rn "sh -c" apps/api/src` 零命中（nft/wg/ip/conntrack 全走 CommandRunner） | ✅ |
| 13 | 无 `nft flush ruleset` / 不修改他人 table | `grep -rn "flush ruleset" apps/api/src` 零命中；仅 `delete table inet floatctf_awd` | ✅ |
| 14 | Firewall 只管理 `table inet floatctf_awd` | `render.rs::TABLE_NAME` 唯一；`nft list ruleset` E2E 断言（Scenario I/Host Safety） | ✅ |
| 15 | internal 认证常量时间 | `crypto.rs::constant_time_eq` 单测（same/different/length） | ✅ |
| 16 | 私钥一次性返回 | `awd_wireguard_peers.config_fetched_at` + player.rs 二次拉取 403 | ✅ |
| 17 | 状态机守卫 | `transition_event` CAS + `can_transition_to`（awd_transition_guard DB-gated 测试） | ✅ |

## CI 自动执行方式

```bash
# CI-fast 中执行（无 Docker/无 root）：
cargo test --workspace --lib          # 单测（含渲染/状态机/限流/审计断言）
cargo test -p floatctf --test awd_transition_guard   # DB-gated（需 Postgres，CI-docker 层跑）
cargo test -p floatctf --test internal_auth_contract # 源码扫描 + live 401
# 架构 grep（无测试框架的纯断言）：
! grep -rn "vec!\[0u8; 32\]" apps/api/src/bootstrap
! grep -rn "nft flush ruleset" apps/api/src
! grep -rn "sh -c" apps/api/src/modules/event/awd_team
```

## 维护约定

- 新增 internal 端点 → `internal_auth_contract.rs` 的源码扫描自动覆盖（首参断言）。
- 新增敏感 admin 操作 → 必须走 `AuditService::record`（否则 P5-11 grep 检查项失效）。
- 新增 nft 对象 → 只能经 `render.rs` 纯渲染器（所有权铁律）。
