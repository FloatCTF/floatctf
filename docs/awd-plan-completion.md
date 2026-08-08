# AWD 实施计划完成度报告（2026-08-08）

> 对照 [chore/plans/awd/](chore/plans/awd/) 六个 Phase 的执行结果。
> 代码提交均在分支 `awd`；「环境门控」= 需 root/Docker/WireGuard 宿主执行（本机无免密 root）。

## Phase 0 — 安全不变量 ✅ 完成

- `transition_event` 唯一状态入口：事务 + `SELECT FOR UPDATE` + 转移校验 + CAS + `TransitionPatch` 原子字段
- repo 级守卫（update_status/update_phase/mark_verified/clear_verified）；service 层零裸写
- crypto fail-fast（`BootstrapError` → `exit(1)`，零密钥回退删除）
- internal 3 端点 auth 基线（源码扫描 + live 401 双断言）
- 吞错扫描处置（judge_callback 计分显式化、conntrack 显式日志、OnceLock/WG 幂等注释）
- `paused_phase` / `configuration_generation` / `verified_generation` / `config_fetched_at` 迁移

## Phase 1 — native nftables 网络底座 ✅ 完成

- `NftablesFirewallRuntime`（唯一生产实现）：`nft -c` 校验 → 原子 batch → verify；空态删除整个 table
- 纯函数渲染器：三阶段策略 + IPv6 显式占位 + banned set + O(N) 规则；多赛事全局 desired-state reconcile
- 旧 iptables 实现整体删除（firewall.rs / network_policy_service.rs / firewall_cmd）
- `network_runtime = host/noop` 配置；Host capability 判定（缺 nftables → Unsupported）
- IPAM 事务化 + 跨赛事 CIDR 重叠校验（sub-agent）；WG 生命周期闭环 + 私钥一次性（sub-agent + 迁移）
- DeployFailed 写入路径 + infra 容器核验 + recover_all 接入启动（先于 scheduler）
- P1-0 prototype 脚本（root 门控）+ P1-1 host discovery 文档（priority=1 依据）

## Phase 2 — Runtime Precheck + Start Gate ✅ 完成

- precheck 接入 containers/network/firewall/crypto + `ExecutionContext`
- Docker 存活 / SSH（env 门控）/ WG / firewall 结构（revision 比对，Noop 必 fail）/ 矩阵 verify
- host env 快照（P2-13）；flag/judge 隔离探测（precheck 上下文，不污染正式表）
- `configuration_generation` 机制 + touch_configuration + StartBlocked 原因码（AWD_CONFIG_CHANGED）
- 计划开始任务（planned_start_at）

## Phase 3 — 核心闭环 ✅ 完成

- `round_service`：RoundStart→End(Grace)→GraceEnd→RoundStart(N+1) 闭环；幂等（round_number 键，修复 retry 双 round bug）
- phase 切换 = DB desired → 全局 reconcile（Fail Closed → NetworkError + freeze）
- judge deadline 超时计分（-down_points，幂等键）；callback 同身份指数退避重试 + env 白名单（sub-agent）
- realtime 接线：round.started/completed、network.policy.applied/failed、score.changed（DB commit 后发布）
- token rotation 完整编排（key_version+1 + 容器 rollout + 修复主键 bug）；is_valid_token 参与 key_version
- round crash 恢复（restore_round_scheduling 接 recovery）

## Phase 4 — 操作能力 ✅ 完成

- ResetActor 显式化（Player/Admin）；player reset 接完整 execute_reset；limit/protection 强制；penalty 重建成功后扣
- ban 跨层闭环（DB→WG suspend→banned set reconcile→conntrack→publish）+ 自动解封（duration 任务）
- pause/resume 恢复 paused_phase + desired-state reconcile + 网络失败→NetworkError
- finished_at 写入；自动归档（retention_hours）；归档 desired-state 清理（最后赛事删整个 table）

## Phase 5 — 生产验证 ✅ 代码/文档完成，场景执行环境门控

- P5-0 资产恢复 + 审计（e2e_flag_judge.sh 修复；audit 表见 docs/awd-e2e-assets-audit.md）
- Scenario A/E/F DB-gated 自动测试（轮次闭环 / 网络失败 Fail Closed / 多赛事共存）
- P5-10 限流（submit/reset/internal，settings 配置）
- P5-11 管理员审计（TokenRotated/TeamBanned/TeamUnbanned/GameboxReset/ScoreAdjusted）
- P5-12 安全清单可执行化（docs/awd-security-checklist.md，CI grep + 单测）
- P5-14 CI 三层分层（fast / docker / host-network）

## 测试统计（全绿）

```text
cargo test -p floatctf        → 132 lib + 3 scenarios + 5 transition_guard + 14 集成 全过
cargo test -p fcmc            → 15 过
cargo test -p floatctf-awd-judgeserver → 5 过（含 callback 退避 / env 白名单）
cargo check --workspace       → 0 error
```

## 环境门控待办（需 root/Docker 宿主执行）

> 本机无免密 root（uid 1000），以下项无法在本机验证，交付载体 + 前置条件如下。

| # | 待办 | 对应计划项 | 载体 | 前置 |
|---|---|---|---|---|
| 1 | nftables 矩阵实测（Hardening/Attack/Ban/Internet-deny + Docker 共存） | P1-0 | `sudo apps/api/scripts/nft_prototype.sh` | root + nf_tables |
| 2 | host hook/priority discovery 实测（验证 FLOATCTF_FORWARD_PRIORITY=1） | P1-1 | docs/awd-nftables-host-discovery.md §3 命令清单 | root |
| 3 | **P2-6 真实网络矩阵 probe**（方案 A 挂起 WG peers / 方案 B canary namespace；precheck 不得给真实玩家提前攻击窗口） | P2-6 | CI-host-network 或手工 | Docker + WG + root |
| 4 | **P2-7/P2-8 precheck flag/judge 隔离实测**（真实 flag issue + judge callback 链路，X-Precheck-Run 语义，不污染 awd_flag_issues/score） | P2-7/P2-8 | CI-host-network | 容器编排 + root |
| 5 | Scenario A 完整比赛 nft policy transition 实测（DB 级已有：awd_scenarios::scenario_a） | P5-2 | CI-host-network | Docker + WG + root |
| 6 | Scenario B Reset / C Ban（banned set 断言、四入口全拒） | P5-3 | CI-host-network | Docker + WG + root |
| 7 | Scenario D Crash Recovery（任意状态 kill API 重启） | P5-4 | CI-host-network | Docker + root |
| 8 | Scenario G Token Rotation / H Round Transition（幂等：恰一个 Grace、恰一个 next round） | P5-7 | CI-host-network | Docker + root |
| 9 | Scenario I Docker Backend Compatibility（iptables backend 必跑，nftables backend 可选/nightly） | P5-8 | CI-host-network | root |
| 10 | Host Safety（宿主 SSH/无关容器/firewalld/libvirt 不受影响；archive 只删 floatctf_awd） | P5-13 | CI-host-network | root |
| 11 | Flag/Judge Docker smoke | — | `RUN_DOCKER_TESTS=1 apps/api/scripts/e2e_flag_judge.sh` | 补 `Dockerfile.awd-*` |

## 本轮 plan 遗漏修复记录（2026-08-08）

| 提交 | 修复项 |
|---|---|
| ae9cb60 | P4-9：resume 重建 round.end/grace_end 任务（卡死 bug）+ judge_callback 冻结门禁（铁律 #8） |
| 686214c | token 轮换探活（验收 #16）+ recovery 范围扩展 Verified/Deploying/…（验收 #11） |
| b0fc0f1 | **first-blood 并发丢失攻击分 bug**（PG 事务 abort）+ P5-9 三个并发断言 |

> 本轮共确认并修复 **3 个真实代码 bug**（resume 卡死 / judge 冻结期写分 / first-blood abort），
> 新增 5 个 DB-gated 回归测试（pause/resume 重建 + 3 并发 + 既有场景扩展）。

## 与计划的主要偏差（均已注释于代码）

1. `FORWARD_PRIORITY=1` 基于 Netfilter 语义推理选定，需 P1-0 实测确认（docs/awd-nftables-host-discovery.md §3）。
2. judge 超时计分用 `JudgeDown` + reason 标注（score_event_type 枚举无 JudgeTimeout 变体，避免 enum 迁移）。
3. precheck 矩阵验证 = firewall.verify + revision 一致（真实包流 probe 留 E2E，方案 A/B 已文档化）。
4. token rotation 无长窗口新旧并存（DB 原子更新 + 立即容器 rollout；无 schema 支撑双版本存储）。
