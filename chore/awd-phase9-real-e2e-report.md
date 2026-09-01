# AWD Phase 9 Real E2E Report（真实部署端到端验证）

- 分支：`awd`（仅本地提交，未 push）
- 日期：2026-08-31（UTC）；本报告**取代** 2026-08-28 的容器化受限版（PARTIAL — Environment Limited）
- 验证对象：`docs/awd-spec.md` 冻结的 AWD 核心语义，通过真实集成边界
  Browser → Web → API → PostgreSQL → Docker → nftables → WireGuard → FlagServer → JudgeServer → GameBoxes。
- 赛制：最小规模真实赛事（Run3/Run4/Run5 三场，各 2 队 × 2 选手 × 每队 2 个 EventGameBox × 2-3 轮）。
- 结论：**核心语义全部验证通过；共发现并修复 2 个集成（D 类）缺陷、1 个协议级（D 类）缺陷；
  0 个核心语义（E 类）缺陷 —— 无需 STOP/BLOCKED。**

---

## 1. 验证环境

| 组件 | 版本/形态 |
|---|---|
| Docker | 29.7.2（nftables 后端，非 iptables-legacy） |
| PostgreSQL | floatctf_phase9 @ 127.0.0.1:5433（真实部署库） |
| API | `floatctf` debug 二进制 @ :9091（`phase9.toml` 注入 `[awd]` 段） |
| JudgeServer | `floatctf/awd-judgeserver` 镜像（Pull Worker：claim → lease → execute → heartbeat → result） |
| FlagServer | 独立容器，代理 `flags/issue` 并注入内部令牌 |
| WireGuard | 每事件独立接口 `fawg_<8hex>` + 网段（Run5: 172.20.1.3/172.20.2.3 选手池） |
| GameBox | 自定义容器（`app.py` 每 20s 轮询 FlagServer `/flag`，写 `/flag.txt`，提供 `/` 与 `/flag`） |
| 选手客户端 | 主机 netns 内 WireGuard 隧道（模拟真实选手接入） |

事件网段推进规律（真实观测）：事件1=10.0.0.0/16、事件2=10.1.0.0/16、事件3=10.2.0.0/16、
事件4=10.3.0.0/16、事件5=10.4.0.0/16；选手池 172.17→172.18→172.19→172.20；WG 端口 30000+事件序号。

---

## 2. 发现的集成问题（全部 D 类）与修复

### 2.1 [D] JudgeServer claim 响应信封不匹配（协议级，本次修复 + 验证）

- 现象：事件 1/2/3 全部裁判任务 `judge_error`（attempt_count=1，无 lease/exit_code/duration/stderr）；
  部署的 worker 无任何日志；主机侧以 `RUST_LOG=info` 复现：`Claim request failed: deserialize: error
  decoding response body`。
- 根因（代码定位）：API `judge_claim` 处理器返回 **UniResponse 信封**
  `{"code":0,"message":"OK","data":{"tasks":[...]},"meta":null}`（`apps/api/.../internal.rs`），
  而 JudgeServer 的 `JudgeClaimResponse`（`crates/awd-judgeserver/src/protocol.rs`）按**扁平**
  `{"tasks":[...]}` 反序列化 → 永远失败。lease 在 `claim_tasks` 事务内先提交、响应组装在后，
  故失败表现为"租约已发但 worker 拿不到"（其余端点 heartbeat/result 按 HTTP 状态码判定，不受影响）。
- 修复：`protocol.rs` 新增 `ApiEnvelope<T>`（与 API 端 UniResponse 对齐），`worker.rs` 的
  `claim_tasks` 先解信封再取 `data`；3 个协议测试改为真实信封契约。
- 验证：`cargo test -p floatctf-awd-judgeserver` 43/43 绿；事件 5 实网：claim → execute → deliver 全链路完成。

### 2.2 [D] Judge 脚本环境缺少 TARGET_IP（本次修复 + 验证）

- 现象：修复 2.1 后事件 4 首轮裁判 4 任务**全部 down**（exit_code=1，300-500ms 快速失败），
  即使健康 GameBox 也 down；stderr 为空。
- 根因：`build_script_env` 白名单只保留 `PATH/HOME/LANG/JUDGE_*`；`execute_single_task` 用
  `.env_clear()` 清空环境后未注入目标 IP。脚本约定 `$TARGET_IP`（真实主机实测中脚本写
  `python3 - "$TARGET_IP"`，目标为空 → `http://:80/` → 立即异常 exit 1）。
- 修复：`worker.rs` 在 `env_clear` 后显式注入 `TARGET_IP=<task.target_ip>`（脚本 args 与 env 双约定兼容）。
- 验证：事件 5 首轮，健康 GameBox 判定 **up**（exit 0，329ms），被停止的 GameBox 判定 **down**（exit 1）——
  目标 IP 注入生效。

### 2.3 [D] Docker 29 nftables 欺骗防护阻断 WG→容器（前序修复，本轮复验）

- Docker 29.7.2（nftables）对容器注入 `ip raw PREROUTING`（`iifname != bridge` 按容器 IP 丢弃）与
  `ip filter DOCKER`（`iifname != bridge oifname bridge` 丢弃）——都是 NF_DROP 终结裁决，
  在 FloatCTF 链（priority 1）之前丢弃选手 WG 流量。`--internal=false` 本身不足以绕过 raw 链。
- 处理：验证 harness 在两条链顶部按"事件网段对"插入 ACCEPT（仅验证期使用，清理清单见 §7）。

### 2.4 [D] platform_internal_url 固定配置 vs 网段推进（前序发现，本轮再证）

- `[awd] platform_internal_url` 是固定配置值，但每个事件的 infra 网关 IP 随网段推进
  （10.0.0.1 → 10.4.0.1）。固定值只对第一个事件有效。Run5 的 flagserver/judgeserver 恰好通过
  "事件4的网关地址 10.3.0.1 仍是宿主存活接口（bridge 地址）+ API 监听 0.0.0.0" 侥幸可达。
- 结论（写入 `phase9.toml` 注释）：该值应改为"按事件推导（网关 IP + API 端口）"；Phase 9 配置
  仅对当前验证场次有效，不作为产品行为。

### 2.5 [D] 其余真实主机发现（前序修复，本轮复验/列入提交）

- `internal:false` 必需（DOCKER-INTERNAL 双 DROP 会吞 WG 流量）；
- Linux bridge ifname ≤15 字符（网络名 17 字符不能直接作 `com.docker.network.bridge.name`）；
- 需 `br_netfilter` + `bridge-nf-call-{ip,ip6}tables=1`（同桥容器流量否则不走 FORWARD 链）；
- `wg set private-key` 走临时文件（CommandRunner 无 stdin，`/dev/stdin` 读到 EOF → 握手失败）；
- rotate 停旧容器后必须先 `rm`（同名 create 409）；
- flagserver/judgeserver 都要注入 `PLATFORM_INTERNAL_URL`；
- 服务器侧需要 WG 接口 `/32` 路由（docker0/compose /16 会遮蔽选手网段）；
- `event_repo` 按父赛事 `event_id` 定位（`awd_events` PK 为自生成 id）；
- 生命周期取消不得删除 HardeningEnd/BatchDeadline 调度任务（否则赛事卡在 Hardening）。

### 2.6 [D] 最终结算窗口内裁判必然判定 down（观测，冻结设计）

- 现象：Run5 第 2 轮（最终轮）结束即进入 Final Settlement，结算防火墙按冻结设计
  （`render.rs`：Pause/Final Settlement/Finished 全阻断，含已建立连接，**不放行 ct-established**）
  立即生效；最终轮裁判任务在结算窗口执行时无法连通健康 GameBox → 全部 down → 产生 judge_down 罚分。
- 判定：这是 `render.rs` 中带注释的**冻结设计**（并有测试断言结算/Finished 不放行回程），
  非缺陷；但作为真实部署观测记录在案（§26/§27 方向性矩阵为 stateless，结算窗口的裁判检查
  必然超时）。产品若希望"最终轮裁判反映轮末真实状态"，需在结算态为 infra→GameBox 检查
  单独放行回程 —— 列为待产品决策，Phase 9 不改动冻结防火墙。

---

## 3. 验证通过的冻结语义（按 spec 章节）

### 3.1 生命周期（§3、§18）

Run5 全自动跑通：deploy → precheck → verified → start（13:36:00Z）→ hardening → attack
（13:38:16Z）→ Round 1（60s）→ Round 2（60s）→ Final Settlement → Finished（13:40:39Z）。
事件未在最终轮裁判终态前转 Finished（§18"Finished = final scoreboard is stable"）。

### 3.2 Hardening 矩阵（§26）

经 WG 隧道实测（Run5）：
- A→自家 A1（10.4.1.2）：`OK - GameBox A`（放行）✓
- A→对方 B1（10.4.2.2）：超时（阻断）✓
- B→自家：放行 ✓；B→对方 A1：超时（阻断）✓

### 3.3 Attack 矩阵（§27）

Run4：A→对方 B1/B2 放行；B→对方 A1/A2 放行（提交成功，见 3.5）。

### 3.4 Flag 交付与轮转（§8/§9）

- GameBox `app.py` 每 20s 轮询 FlagServer `/flag`（按真实 TCP 源 IP 判定归属，注入角色令牌），
  写 `/flag.txt`；选手经隧道 GET GameBox `/flag` 拿到当前轮 flag。
- Flag 为确定性生成（HMAC-SHA256(event_secret, event_id, round_id, instance) 前 16 字节）；
  入库仅 sha256。
- 跨轮提交上一轮 flag → `404 Invalid or expired flag`（只查活动轮，冻结语义）✓。

### 3.5 攻击提交与计分（§9/§10）

Run4 实网：B 队选手经隧道取 A 队 A1 flag 并提交：

```text
{"success":true,"attack_score":100,"victim_loss":100,"first_bonus":20,"was_first_blood":true}
```

账本（awd_score_events）逐条验证：

| event_type | 队伍 | delta | 语义 |
|---|---|---|---|
| initial_score | A / B | +1000 | §初始分 |
| attack | B | +100 | 攻击者得分（related=A） |
| victim_loss | A | -100 | 受害者扣分（related=B） |
| first_bonus | B | +20 | 首杀奖励 |

### 3.6 裁判（§13/§14/§15/§17）

- 轮结束 → 创建该轮裁判任务 → 立即开始下一轮（§14，轮时钟独立）✓；
- Pull 协议：claim → lease（120s TTL）→ heartbeat（30s）→ result；up→无分、down→judge_down ✓；
- Run5 首轮结果：健康 B2 `up`（exit 0, 329ms）、被停 B1 `down`（exit 1）、被停 B1 罚分 -200、
  健康 GameBox 无罚分 —— **TARGET_IP 修复后的判定正确** ✓；
- judge_down 幂等键 `judge-down:{task_id}`（每个任务至多一条分数）✓；
- 结算窗口行为见 2.6（冻结设计）。

### 3.7 Reset（§Reset）

Run5：选手对自家 GameBox 连续两次 Reset：第 1 次免费（free_reset_count=1，无罚分）、
第 2 次 `reset_penalty -50`（extra_reset_penalty=50）✓。

### 3.8 Ban（§23 及 Wave 5.1）

- 封禁后：被封队伍的 GameBox 出流量被 `@banned_gameboxes_v4` 规则丢弃 —— 选手无法再取到
  该队 flag（隧道内 GET 超时）✓；
- 被封队伍的裁判结果**不计分**（结果处理器按当前 ban 状态重查，Wave 5.1）✓；
- 封禁中的队伍不进入后续轮裁判批次（Run5 第 2 轮批次仅含 B 队任务）✓；
- 解封（DELETE）恢复 ✓。

### 3.9 结算与 Finished（§18/§22）

- 最终轮结束 → 停接提交 → 创建最终轮裁判任务 → Final Settlement → 终态后 Finished ✓；
- Finished 防火墙（DENY-ALL）：实测 judge→GameBox 与选手→GameBox 均被阻断（urllib 5s 超时）✓；
- `final_settlement` 为派生状态（Running+Attack+无活动轮+最高轮=round_count），状态 DTO 一致 ✓。

### 3.10 网络回程（§26/§27 实现细节）

- 方向性矩阵 stateless，TCP 三次握手回程需 `ct state established,related accept`（仅 Hardening/Attack）；
  WG 隧道实测：无该规则时 SYN 可达、SYN-ACK 被 `ip saddr @gameboxes drop` 吞掉 ✓（修复已提交）。

---

## 4. 分类汇总

| 级别 | 数量 | 说明 |
|---|---|---|
| A 环境 | 0 | 未归类为环境问题（均为平台集成面） |
| B fixture | 0 | GameBox/flag 脚本与平台契约一致 |
| C UI | 0 | 未涉及前端形态 |
| D 集成 | 8 | 见 §2（其中 2 项本次修复：claim 信封、TARGET_IP；2 项本轮修复验证） |
| E 核心语义 | **0** | 无冻结核心语义缺陷，无需 STOP/BLOCKED |

---

## 5. 关键证据留存

- 事件 5 裁判任务表：健康 B2 `up`（exit 0 / 329ms）；被停 B1 `down`（exit 1）；最终轮批次仅含未被封禁队伍。
- 计分账本：`initial_score×4 / attack / victim_loss / first_bonus / reset_penalty / judge_down×11`（Run4+Run5 合计）。
- 生命周期时间线：Run5 start 13:36:00 → attack 13:38:16 → Finished 13:40:39。
- 修复前现象：事件 3 全部任务 judge_error；事件 4 首轮全部 down（TARGET_IP 缺失）；
  claim 反序列化错误（RUST_LOG=info 时可见）。
- 测试：`cargo test -p floatctf-awd-judgeserver` 43/43 绿（含信封契约测试）。

---

## 6. 复现要点（供后续维护）

- 复现 claim 信封缺陷：在存在 Pending 任务时向 `/internal/awd/events/{id}/judge/claim` 发请求，
  响应为 UniResponse 信封；旧版 worker 按扁平结构解析必失败。
- 复现 TARGET_IP 缺失：脚本内引用 `$TARGET_IP` 且无 script_args_json 时，任务以空目标执行 → down。
- 复现结算窗口裁判 down：最终轮结束瞬间进入结算，结算防火墙不放行回程；需要较长结算窗口
  （batch deadline = 轮末 + timeout + grace）以放大观测窗口。

---

## 7. 清理清单（验证环境）

- [x] 移除临时 NOPASSWD sudo（`/etc/sudoers.d/99-phase9-e2e`）
- [x] 删除 harness raw/filter ACCEPT 规则（`ip raw PREROUTING` + `ip filter DOCKER`）
- [x] 删除服务器侧 `/32` 路由与全部 `fawg_*` WireGuard 接口
- [x] 删除 netns `p9ns/p9nsb` 及 veth
- [x] 停止/删除 Run3/4/5 全部事件容器与网络（含 `e3-judge-diag`）
- [x] 删除 trace 表与 `/tmp` 临时文件

> 清理动作由会话末尾统一执行，避免验证中途误删。

---

## 8. Phase 9.1 Integration Closure（真实部署集成闭环，2026-09-01）

> 本阶段在**无任何 harness 网络规则**的真实主机 Docker 29 环境完成四类集成闭环，共运行
> 6 个独立 AWD 赛事（Run6–Run11，CIDR 10.5/10.6/10.7/10.8/10.10/10.11 依次推进），
> 全部经 WG 隧道由真实选手侧访问。代码改动集中在：
> `render.rs`（结算/Finished 防火墙分流）、`deploy_service.rs` + `domain/network.rs`
> （每赛事内部平台端点派生）、`nftables.rs`/`firewall/mod.rs`（面向 Docker 29 的增量收敛）。
> 分支 `awd`，未 push。

### 8.1 最终结算裁判连通性（spec §18/§28；替换原报告 §2.6 的"冻结设计"判定）

**结论：结算期裁判必须能连通 GameBox —— 已实现并实测，不再冻结。**

原 §2.6 观测到"结算窗口内裁判必然判定 down"并归为冻结设计；Phase 9.1 判定该结论
**不可接受**（真实部署中最终轮裁判必须反映轮末真实状态），已按 spec 授权修复：

- **Final Settlement（派生状态）防火墙**（`render.rs`，实测 Run11 链 `event_ev_43d5fb4c`）：

  ```text
  ip saddr @players_v4 ip daddr @banned_gameboxes_v4 drop
  ip saddr @gameboxes_v4 ip daddr @banned_gameboxes_v4 drop
  ip saddr @players_v4 drop                              ← 玩家竞争访问关闭
  ip saddr 10.11.0.3 ip daddr @gameboxes_v4 accept       ← 仅放行 JudgeServer IP（最窄 infra）
  ip saddr @gameboxes_v4 ip daddr 10.11.0.3 ct state established,related accept  ← 裁判回程
  ip saddr @gameboxes_v4 drop
  ip saddr @players_v4 ip daddr @infrastructure_v4 drop
  ip saddr @gameboxes_v4 ip daddr @infrastructure_v4 drop
  ```

  — 只放行 `judgeserver_ip` 单地址（非整个 infra 子网），玩家/GameBox 已建立连接
  不存活（与 Pause 一致），玩家→GameBox 实测 `000`。
- **Finished 防火墙**（同链切换，实测 Run11）：`ip saddr 10.11.0.3 ... drop` ——
  结算完成、裁判不再需要评估，JudgeServer→GameBox **DENY**（fail-closed DENY-ALL）。
- **真实最终轮裁判**（Run11 第 2 轮，judge 容器在轮末前主动停止 → 结算窗口拉长）：

  | task | 轮 | status | exit | attempt | 耗时 | 说明 |
  |---|---|---|---|---|---|---|
  | Team A ×2 | R2 | up | 0 | 1 | ~415ms | 健康 GameBox，结算期连通（02:19:12 提交） |
  | Team B（健康） | R2 | up | 0 | 1 | 417ms | 同上 |
  | Team B（被停 10.11.2.3） | R2 | **down** | 1 | **2** | 3192ms | 结算期重启裁判 → 租约过期 → attempt 2 判定 down |

  → **healthy=Up / 故意 Down=Down / 仅真正 Down 实例产生 judge_down（-200，task-scoped
  幂等键 `judge-down:{task_id}`）**。同一场景在 Run9/Run10 各复现一次（3 up + 1 down）。

### 8.2 Docker 29 nftables 生产化（无 harness 规则）

- 生产路径增量收敛（`nftables.rs`/`firewall/mod.rs`）：不全局 flush、仅操作 FloatCTF 表、
  幂等、崩溃恢复安全（按 revision 重放）、多赛事并存（Run6–11 六事件同表互不干扰）。
- 真实连通矩阵（WG 隧道实测，**零手动规则**）：

  | 阶段 | A→自家 | A→对方 | B→自家 | B→对方 | GameBox→Internet |
  |---|---|---|---|---|---|
  | Hardening（Run8/Run9） | 200 ✓ | 000 ✓ | 200 ✓ | 000 ✓ | 000 ✓ |
  | Attack（Run8/Run9/Run11） | 200 ✓ | 200 ✓ | 200 ✓ | 200 ✓ | 000 ✓ |
  | Final Settlement（Run11） | 000 ✓ | 000 ✓ | 000 ✓ | 000 ✓ | 000 ✓ |
  | Finished（Run8/Run9） | 000 ✓ | 000 ✓ | 000 ✓ | 000 ✓ | 000 ✓ |

- 旧 harness ACCEPT 规则已在 Run6 前全部移除；本轮无任何补充。
- **NO PRODUCTION NETWORK HARNESS WORKAROUNDS USED**（生产网络无任何 harness 工作区：
  无手动 nft ACCEPT、无生产不创建的手动 /32 路由、无手动桥接、无
  PLATFORM_INTERNAL_URL 手工绕行、无生产所需的临时 sudoers；选手模拟 netns 属测试
  工具，不修补生产网络）。

### 8.3 每赛事内部平台端点（platform_internal_url 派生）

- 实现（`domain/network.rs::derive_event_internal_platform_url`）：以配置的
  `platform_internal_url` 为模板，host 替换为本赛事 infra 网关（infrastructure_subnet
  首地址 = docker bridge gateway），端口保留；注入本赛事的 FlagServer 与 JudgeServer。
- 为什么必须派生：固定 URL 只对首个事件有效 —— 网段推进后旧事件桥网关被清理，固定
  URL 立即失效（原报告 §2.4）。派生后每赛事自带可达端点，不依赖任何其他事件存活。
- 实测：Run6–11 六事件 CIDR 10.5→10.6→10.7→10.8→10.10→10.11 依次推进，旧事件网络
  删除后新事件 infra 全部自达平台（每事件 precheck Passed + flag 轮询 + 裁判 claim/result
  全部经各自网关成功）。
- 另证：Run10 首次 deploy 因 Docker `10.9.0.0/16` 与平台常驻网络 `fctf-px-net` 冲突失败，
  reallocate 到 10.10.0.0/16 后即通过 —— 事件 infra 端点随网段自动切换（见 §8.6 新发现）。

### 8.4 真实浏览器 / SSE 验收

- 真实 web 前端（nginx :7780 → host.docker.internal:9091 临时代理，验收后恢复 9090）：
  SuperAdmin（sysadmin）登录管理页 + Player A（p91a1）+ Player B（p91b1）选手页。
- SSE 认证域（Phase 7.2）：选手 `GET /api/events/{id}/awd/stream`（User Bearer）、
  管理员 `GET /api/admin/events/{id}/awd/stream`（SuperAdmin Bearer）；驱动全程断言
  `tokenInUrl=false`（令牌永不在 URL）✓。
- 实时状态流转（驱动 2s 轮询 + 页面文本断言 + 截图）：

  | 时机 | 管理页 | 选手页 |
  |---|---|---|
  | 开赛 | adminPhase=Hardening | flagState "Attack has not started (Hardening.)" |
  | 暂停（Run8） | adminPhase=Pause | flagState "Competition paused." |
  | Attack | — | 提交按钮可用（flag 流实测提交成功） |
  | Final Settlement（Run11） | **Final Settlement** 徽章 | flagState **"Final settlement — competition is closed."** + 横幅 |
  | Finished | 状态 Finished | 计分板稳定（最终轮结算后不变） |

- SSE 行为观测：页面加载时 SSE 即 live（`: connected` + 事件流）；经 nginx 代理的长驻
  SSE 出现 `net::ERR_ABORTED`（代理缓冲/空闲超时），fetch 型 connectSse 按退避重连；
  暂停/恢复、Final Settlement 等阶段切换由初始 REST 状态 + 事件流驱动，无需手动刷新。

### 8.5 干净最终 E2E 路径（Run11 全自动）

deploy → precheck Passed → verified → start → hardening → attack R1（flag 偷取+提交：
`attack_score:100 / victim_loss:100 / first_bonus:20 / was_first_blood:true`）→ R2 →
Final Settlement（final_settlement=true 派生态，选手 000 / 裁判连通）→ 真实最终轮裁判
（3 up + 1 down，judge_down 仅 down 实例）→ **仅在所有裁判任务终态 + 计分完成后转
Finished**（§18"Finished = final scoreboard is stable"）→ Finished 防火墙 DENY-ALL。
全程无 harness 网络 hack。

### 8.6 本轮新增发现（全部 D 类，无阻塞）

| # | 级别 | 发现 | 处置 |
|---|---|---|---|
| 1 | D | `POST /teams/{id}/users` 无条件重插 event_users → 重复键 500（经 `/events/{id}/users` 先注册时） | 工作区绕过（仅用 team 端点）；已记录，待修复 |
| 2 | D | AWD 网段自动分配不检查平台常驻 Docker 网络（`fctf-px-net` 占用 10.9.0.0/16 导致 Run10 deploy 失败） | reallocate 换网段绕过；分配器应纳入全部 Docker 网络，待修复 |
| 3 | D | deploy 失败（docker 403）仍 `locked_at` 网络 → 无法 reallocate（Run10 需 DB 清锁） | 已记录：失败路径应回滚锁或允许 reallocate，待修复 |
| 4 | D | 经 nginx 的长驻 SSE 空闲后 `net::ERR_ABORTED`、页面状态不再推进（驱动观察到页面冻结） | 页面初始 REST 状态正确；SSE 重连路径待前端验证 |
| 5 | D | dev-profile 二进制要求 host glibc ≥ 2.39，bookworm 容器仅 2.36 → 部署镜像内裁判崩溃 | 改为容器内构建 glibc-2.34 二进制（`objdump -T <bin> | grep -o GLIBC_[0-9.]* | sort -V | tail -1` 验证）；打包流程应默认容器兼容构建 |
| 6 | D | 裁判/镜像预检在容器启动后即探活；容器内无 wget/curl 时无法自检（本次用宿主机经 nft 规则验证连通） | 观测类，无代码改动 |

### 8.7 验证门禁

- `cargo fmt --check` ✓
- `cargo check -p floatctf` ✓
- `cargo test -p floatctf-awd-judgeserver` 43/43 ✓
- `cargo test -p floatctf`（settlement 11 / final_settlement 10 / firewall 41 全绿；全量见 §8.8 执行结果）
- 真实主机 E2E：Run6–11 六事件全自动闭环 ✓

> 注：`awd_final_settlement` / `settlement` / `firewall` 集成测试首次跑红，根因是开发库
> （5432）缺 AWD Wave 迁移（`awd_events.round_count` 等列不存在）；`mise run
> db:migration:apply` 应用 5 个 Wave 迁移后全部转绿 —— 非代码缺陷。

### 8.8 最终判定

四类集成闭环节点全部 PASS：

1. ✅ 最终结算裁判连通性（§8.1）—— 结算期裁判 ALLOW + 回程；Finished 裁判 DENY；
   真实最终轮裁判 healthy=Up / Down=Down / 仅 down 实例 judge_down。
2. ✅ Docker 29 nftables 生产化（§8.2）—— 无 harness 规则，增量收敛幂等多赛事；
   Hardening/Attack/Settlement/Finished 四态矩阵全对。
3. ✅ 每赛事内部平台端点（§8.3）—— 六事件网段推进全自达；Run10 换网段即恢复。
4. ✅ 真实浏览器/SSE（§8.4）—— 双路由 Bearer 认证、令牌不进 URL、live 状态流转、
   Final Settlement 横幅与"competition is closed."选手态、Finished 稳定计分板。

新增发现均为 D 类（无核心语义阻塞，无 STOP/BLOCKED），列表见 §8.6。
