# P5-0 已有测试资产审计（2026-08-08）

> 5 个 E2E/fault/load 资产从 git 历史（commit `9e0df54` 删除前）恢复并审计。
> 标注：`USABLE` / `EXISTS_BUT_PARTIAL` / `STALE` / `BROKEN`。
> 恢复文件已按当前仓库结构修正引用（bin 已迁移为独立 crates）。

| 资产 | 状态 | 审计结论与处置 |
|---|---|---|
| `apps/api/scripts/e2e_flag_judge.sh` | **STALE → 已修复** | 引用 `cargo check --bin awd_flagserver`（旧 bin 结构）→ 改为 `-p floatctf-awd-flagserver/judgeserver`；ROOT 修正到仓库根；Dockerfiles 不存在时降级为 cargo-check + 显式 SKIP。**Docker smoke 路径依赖补上 `Dockerfile.awd-*`（crates/ 下）**。 |
| `apps/api/scripts/fault_injection_checklist.md` | **USABLE** | 10 场景手工验收清单；通用性良好。个别措辞仍引用旧 iptables/WG TODO → 已更新为 nftables desired-state 语义（见文件内 "Network / phase checks"）。 |
| `apps/api/scripts/load_smoke.sh` | **USABLE** | BASE_URL 门控，CI 安全跳过；hey/ab/curl 三驱动；无需改动。 |
| `apps/api/docker-compose.e2e.yml` | **EXISTS_BUT_PARTIAL** | 引用 `Dockerfile.awd-*`（当前缺失）与 `PLATFORM_INTERNAL_URL`；待 Dockerfiles 就位后可用。 |
| `apps/api/docs/e2e-flag-judge.md` | **EXISTS_BUT_PARTIAL** | bin 名引用已修正；Docker smoke 说明与脚本行为一致。 |

## 修复记录

- `e2e_flag_judge.sh`：cargo target 从 `--bin` 改为 `-p floatctf-awd-*`；ROOT 上移一级；
  Dockerfiles 缺失时不再硬失败，降级 cargo-check + 显式 SKIP（CI 安全）。
- `e2e-flag-judge.md`：同步 cargo 命令。
- `fault_injection_checklist.md`：网络/phase 检查项措辞对齐 nftables（banned set / desired-state reconcile）。

## 待办（依赖环境）

- [ ] 补 `Dockerfile.awd-flagserver` / `Dockerfile.awd-judgeserver`（crates/ 构建上下文）→ 解锁 Docker smoke 与 compose profile。
- [ ] Scenario A~I E2E（P5-1..P5-8）在 root + Docker + WireGuard 环境执行（见 `scripts/nft_prototype.sh` 与下方 Scenario 测试骨架）。
