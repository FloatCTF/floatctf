# FloatCTF 全量测试矩阵与修复清单

> 方式：**真实浏览器**（Edge + CDP，真实登录态、真实点击/填表/确认框）为主，配合真实管理接口做批量数据准备与并发压力；所有结论都有数据库/服务端日志/接口响应作为证据。
> 环境：生产部署（`/home/fb0sh/floatctf-prod`，API `floatctf/api:0.3.3`，Caddy + PostgreSQL 5433 + Redis + RustFS），宿主 12C/15G。
> 覆盖：练习（Jeopardy / AWDP）、管理（16 个页面 + 赛事四类 family 管理页）、比赛（多用户多题目全生命周期）+ 边界 + 并发 + 压力。

---

## 一、测试矩阵（结果）

### 1. 练习

| 场景 | 操作 | 结果 |
|---|---|---|
| Jeopardy 练习可见性 | 玩家 `/service/challenges` | 看到全部非隐藏题目 ✓ |
| Jeopardy 启动实例 | 题目详情 → Start | 实例启动，出现 flag 输入与附件 ✓ |
| Jeopardy 错误 flag | 提交 `flag{wrong}` | 拒绝，solve 数不变 ✓ |
| Jeopardy 正确 flag | 提交静态 flag | 通过，solve +1，实例自动销毁 ✓ |
| Jeopardy 复练/重复提交 | 重新 Start 后再提交 | 允许复练；solve 不重复计分（幂等）✓ |
| AWDP 练习启动 | `/service/gameboxes` → test-g → Launch | 练习 Run 建立，工作台可用，端点可达 ✓ |
| AWDP Break 边界 | 容器内取 flag → 错误/正确提交 | 错误拒绝；正确 `+score`，0 → **540**，Unbroken → Broken ✓ |
| AWDP Fix 语义 | 切 Fix 阶段 → Test/ALL Check | 官方回合评测 `no_patch`（未提交补丁不计分）✓ |
| 练习赛事件页 | 直接访问 `/service/events/awdp/<practice>` | **F23 已记录**：玩家端 404/403 时页面卡 Loading（入口实为 GameBoxes）|

### 2. 管理（16 个侧边栏页面 + 赛事管理页）

| 页面 | 结果 |
|---|---|
| Dashboard / Super Admins | 渲染正常，1 条超管 ✓ |
| Users | 增删改查 + 边界全通过（见 F20/F21）✓ |
| All Events | 创建（含赛制/参赛模式/时间校验）、编辑、删除、对话框中文标签 ✓ |
| Challenges | Scan/Check/删除；12 道题 ready ✓ |
| Challenge Sets | 创建/编辑/删除全通过 ✓ |
| GameBoxes | 列表 + 挂载到 AWDP 练习 ✓ |
| Weapons / Announcements / Discussions | 渲染正常（空表）✓ |
| AWD Networking / AWD GameBoxes | 平台与赛事网段配置、校验边界全部通过 ✓ |
| Terminal / Docker / Database | 渲染正常（Docker 10 条）✓ |
| Logs | 渲染正常（10 条/页）✓ |
| Scheduled Tasks | 分类正确（SystemTask 6 + EventTasks 7 = 库内 13）✓ |
| Settings | 12 项设置与解析值正确 ✓ |
| Version | 版本信息正确（api 0.3.3）✓ |
| 赛事管理（jeopardy/awd/awdp） | 挂题/发布/分值/预检报告/网络/队伍/实例/日志/WriteUp 页面渲染与交互 ✓ |

**管理端关键 CRUD 证据**
- 用户：空/空白用户名、空密码、短密码、非法邮箱 → 400 中文提示；重复用户名 → **409**；空凭据登录 → **401**（修复前 200 + JWT）。
- 题目：目录 Scan 注册 → 按规范 tag 构建镜像 → 状态 `ready`；`failed` 状态无法通过 Scan/Check 修复（**F28**，需删除后重扫）。
- 教材集：创建 → 行内编辑写库 → 删除，三步均通过。
- 赛事：创建（Jeopardy 个人赛，允许加入）、挂 12 题（UI 挂 10 + 接口补 2）、Open Challenges 发布、对话框必填拦截与中文标签。

### 3. 比赛（多用户多题目）

| 场景 | 规模 | 结果 |
|---|---|---|
| 批量用户 | 30 个真实账号（管理接口） | 全部创建成功 ✓ |
| 加入风暴 | 30 用户 × 2 并发 | **全部 200**，p95 156ms，参与者恰好 30（幂等）✓ |
| 首杀竞态 | 5 用户同时提交同题 | **全部 200**，墙钟 0.285s，动态分合计 499.45（无重复计分）✓ |
| 去重竞态 | 同用户 20 并发提交同实例 | **全部 200**，DB 恰好 **1 条** solve ✓ |
| 轮询负载 | 5 端点 × 25 并发 | **全部 200**，p95 ≤ 96ms，无 5xx ✓ |
| 浏览器解题 | 加入 → 打开题目 → Launch → 错/对 flag | 错误拒绝；正确「提交成功」并计 100 分 ✓ |
| 未开赛可见性 | 赛事 start 前访问 Challenges | 「赛事尚未开始」（题目不外泄）✓ |
| 开赛后加入 | 已开赛事件申请加入 | 400「赛事已开始，无法加入」✓ |
| 隐藏题目 | 赛事内 2 道保持 hidden | 玩家不可见 ✓ |

### 4. 压力/资源结论

- 单实例提交链路：p50 104–235ms（并发 20–60 时无错误）。
- 读路径：赛事详情 p50 13ms、排行榜 29ms、题目列表 88ms（25 并发）。
- 清理了 **173 个测试残留容器 + 107 个残留网络**（全量回归的 AWDP 真实宿主用例产物，非产品泄漏），宿主可用内存 7G → 9G。

---

## 二、修复清单（本轮 + 上一轮，均已部署验证）

| # | 问题 | 影响 | 修复 | 提交 |
|---|---|---|---|---|
| **F35** | 提交 flag 时省略 `event_id`，竞赛实例被当作练习路径写入 **0 分** solve；`already_solved` 永久挡住补救 | **分数完整性**：选手永久拿不到该题分数 | 省略 event_id 时按实例归属解析赛事；练习路径按 `purpose` 判定并拒绝竞赛实例 | `1b40055` |
| **F25** | API 镜像只装 `python3-minimal`，无 json/urllib 标准库 → 判题脚本必然 ModuleNotFoundError | 所有 manual 评测失真为 service_down / 假「修复成功」 | 镜像改装完整 `python3`（含 install.sh 内嵌模板） | `04219cc` |
| **F34** | 重复/并发加入赛事 → 唯一约束 500 | 并发下用户看到「服务器内部错误」 | 幂等：先查既有报名，冲突读回该行 | `606c04e` |
| **F20** | 用户对话框**全空提交成功**建号，且**空用户名+空密码可登录**（HTTP 200 + JWT） | **安全**：可预测凭据账号 | 校验用户名/密码/邮箱 + 单元测试 | `8d3a868` |
| **F21** | 重复用户名 → 500 且回传约束名 | 体验 + 信息泄漏 | 唯一冲突映射 409「用户名已被占用」 | `8d3a868` |
| **F33** | 已开赛拒绝加入/退出文案写成 `Event has not yet begun`（语义颠倒） | 误导用户 | 改为「赛事已开始，无法加入/退出」 | `606c04e` |
| **F27** | 服务不可达时工作台显示「修复成功：True」 | 误导选手 | 三态：无法判定 / 已修复 / 未修复 | `1c3f4bd` |
| **F11** | 归档赛事不释放平台网段（`release_allocations` 无生产调用方） | 地址池泄漏，16 个赛事后无法再分配 | 归档收尾释放 + 重复归档自愈 | `c0c9473` |
| **F17** | 重复提交 flag 被 SeaORM 事务拍平成 500 | 用户看不到真实原因 | 用 `TransactionError<AwdError>` 保留业务冲突 | `0b4ccf4` |
| **F14** | 重名建队 → 500 + 回传数据库约束名 | 体验 + 信息泄漏 | 映射 400「队伍名称已存在」+ 用例 | `e8b7f00` |
| **F15/F18** | 数据库错误细节回传客户端；错误文案带 `Not found:` 等英文前缀 | 信息泄漏 + 中英夹杂 | 详情只进日志；只返回业务文案 | `f8154a2` |
| **F3** | AWDP 判题容器名冲突只重查一次就放弃 | 练习环境长期 ensure 失败 | 5 次退避重试 + 兜底清理重建 | `4f9067b` |
| **F4** | 空标题赛事可创建；对话框字段标签是原始键名；失败提示被抽屉遮挡；Update 失败仍关窗 | 数据质量 + 可用性 | 必填拦截 + 中文标签 + 抽屉内横幅 + 成功才关窗 | `e8b7f00` `ce2a596` |
| **F12** | 未开赛赛事显示「Hardening」（phase 默认值先于 status 判断） | 误导组织者 | 状态优先级 + 抽 `progressDisplay` 纯函数与回归用例 | `34a9288` |
| **F22/F31** | 提交反馈与玩家端文案英文（wrong flag / Flag is correct! / You are not joined / Event not found） | 中文平台体验 | 全部中文化（含后端 25 处玩家端文案） | `9362176` `1b40055` |
| **F1/F2/F7** | 登录失败显示 axios 英文原文；表单缺 autoComplete；横幅标题硬编码 `title` | 体验 | 统一 `formatApiError` + 中文兜底 + 无障碍 | `badaaf7` |
| **F10** | 手动分配网段错误示例是不可用的 `/24` | 照抄即报错 | 示例改 `/16` | `0fefcd0` |
| **F16** | AWD 配置页/运维页/玩家端全英文 | 可用性 | 全面中文化 | `1e7b6b0` |
| **F28** | 题目 `build_status=failed` 后 **Scan 不重导入、Check 只校验不回写**，管理员只能删库重扫 | 可维护性 | Check 在镜像可 inspect 时把状态修回 `ready` 并 pin image_id、清空 build_error | `6fe9526` |
| **F36** | 除用户/赛事外，其余管理端对话框字段标签仍是原始键名（`name`/`content`/`task_key`…） | 可用性 | 10 个页面共 73 处补中文标签 | `6fe9526` |
| **F38** | AWDP 配置页/运维页文案英文（`Fix Duration`/`Turn Interval`/`Save AWDP Configuration`/`AWDP configuration saved`/`Flag rejected`…） | 可用性 | **待修**（未提交）|
| **F40** | 竞赛模式下补丁 `applied` 后后续回合仍判 `no_patch`，修复方拿不到 fix 分数 | 计分语义 | **待确认**（见第六章证据）|
| **F37** | 定时任务可注册**未知 task_key**、`cron` 不填表达式、`once` 不填执行时间（永远不会执行） | 可维护性 | 任务键必须命中 `TaskKey` 注册表；触发方式白名单 + 必需字段 + Cron 可解析校验（创建与更新共用） | `afa0ce3` |

---

## 三、仍待决策（架构级，未改动）

1. **F26：manual「Test Check」在生产必然失败**。`run_script` 在 **API 容器内**执行判题/利用脚本，而生产 API 只在 `fctf-platform-control`（10.42.8.2），跨 docker 网络被隔离 → 必然超时。official 回合评测由数据网内的 JudgeServer 领取执行（正常）。可选方案：(a) manual 检查改为投递给 JudgeServer；(b) 降低/移除该按钮语义。
2. **F24：练习实例被分配到网络地址 `.0`**（`IPRange 10.42.3.0/24`，容器实际 IP `10.42.3.0`）。宿主可达、容器间不可用。建议把 IPRange 起始改为 `.2`。

## 四、已知覆盖缺口

| 项 | 原因 |
|---|---|
| GameBox / Challenge **zip 上传导入** | 文件选择器在 Windows 侧，CDP 无文件上传能力；改用服务器目录 + 真实 Scan 流程覆盖注册/构建路径 |
| AWDP **patch.tar.gz 上传**与正式计分 | 同上；已用容器内直接修复 + 官方回合评测语义验证 |
| `awd_live` 真实宿主归档用例 | 默认被 `#[ignore]`（需隔离 PG + helper + Docker/WireGuard/nftables），已在生产实测 F11 |
| 跨轮旧 flag 拒绝 | 需 ≥2 回合赛事；已有 `awd_submission_boundaries` 集成用例覆盖（9 例全通过）|

## 五、管理端补充实测（第 3 轮）

| 项 | 操作 | 结果 |
|---|---|---|
| 公告 CRUD | 新增（title/content）→ 库与列表一致 → 删除 | ✓ |
| Weapons CRUD | 新增（名称/分类/描述）→ 库一致 → 删除 | ✓ |
| 题目集 CRUD | 新增 → 行内编辑写库 → 删除 | ✓ |
| Discussions（管理） | 页面无新增入口（讨论由玩家发起，管理端仅查看/审核） | ✓ 符合设计 |
| Scheduled Tasks | 分类正确（6+7=13）；**未知任务键/cron 缺表达式/once 缺执行时间/非法 cron → 400**，合法任务 → 200 | ✓（F37 修复后）|
| Challenges Check | 手工置 `failed` → 浏览器点 Check → `ready` + image_id pin | ✓（F28 修复后）|
| 对话框标签 | weapons/scheduled_tasks/challenge_sets 等抽查均为中文 | ✓（F36 修复后）|

## 六、AWDP 竞赛全生命周期（第 4 轮补充）

| 步骤 | 操作 | 结果 |
|---|---|---|
| 建赛 | 浏览器新建 `AWDP 竞赛压测`（family=awdp / team / 允许加入） | ✓ |
| 配置 | `/admin/events/awdp/$id/configure`：Fix 600s、Turn Interval **60s**、Break 90、Fix/Turn 150 | ✓ 落库一致 |
| 挂靶机 | `Attach GameBoxes` → test-g | ✓ |
| 组队 | API 建 AWDP-A（qa01+qa02）、AWDP-B（qa03+qa04） | ✓ 各 2 人 |
| 开始 Break | 运维页 `Start (→ Break)` | ✓「操作成功 Started → Break」，两队实例自动创建并运行 |
| 真实 Break | 本队容器内取 `judge-server/flag` → 页面提交 | 错误 flag 拒绝；正确 `Flag accepted, +score`；Unbroken → **Broken** |
| 计分/积分榜 | `awdp_score_events: break +90`；玩家积分榜 **1 A AWDP-A ME 90 0 90**，AWDP-B 0 | ✓ |
| 切 Fix | 运维页 `Break → Fix` | ✓「操作成功 …(all instances reset)」，实例重置为新 flag |
| Fix 回合 | 每 60s 生成回合（sequence 1..8+），官方评测按轮执行 | ✓ 语义 `no_patch`（未提交补丁）正确 |
| 真实修复 | 容器内改写 `index.php` 阻断 SSRF | ✓ 攻击面 `?url=http://judge-server/flag` → **400 blocked**；健康检查路径 `?url=http://127.0.0.1` 仍 **200** |
| 补丁提交 | 构造 `patch.tar.gz`（根 `patch.sh`）→ 玩家接口上传 | ✓ `{"status":"applied"}`，`awdp_patch_submissions` 记录 applied/exit=0 |
| 结束 | 运维页 `Finish (→ Ended)` | ✓ 中文确认框「确认结束赛事？…剩余评估仍会结算」，phase=**ended** |

**F40（已定论：非缺陷）**：补丁 `fix_round_id` = 第 4 回合（04:43:53~04:44:53），第 4 回合评测（04:45:00）**确实找到了补丁**（否则应为 `no_patch`），但因该实例当时不可达而以 **`service_down`** 结算；第 5/6 回合按其自身 `round_id` 检查补丁 → `no_patch` 属**设计内语义**（`judge_worker` 注释「本轮无 APPLIED patch 短路」，plan §22）。结论：竞赛模式**按回合**要求补丁；PATCHED 时按「当前轮起全部剩余回合」扫分。本次未得分是因为我的补丁导致实例健康检查失败，而非平台漏判。

**F41（不成立）**：AWDP 赛事 `Finish (→ Ended)` 后，本赛事容器已全部拆除（按 `io.floatctf.event_id` 核对为空）。

**测试卫生（非产品缺陷）**：每跑一轮全量 `scripts/test-rust.sh` 会留下约 35 个容器（AWDP 真实宿主用例的 judge/instance 容器未回收）。本次两批共清理 **173 + 145** 个残留容器与 107 个网络。建议后续给这些用例补 teardown。

## 七、第 5–6 轮补充（管理端收尾）

| 项 | 操作 | 结果 |
|---|---|---|
| Super Admins | 新建 `qaadmin` → **可登录**（200 + JWT）→ 行内删除 → 回到 1 条 | ✓ |
| Settings 编辑 | `INSTANCE_DESTROY_DELAY` 60 → 61 写库 → 回滚为 60 | ✓（生产配置改后即复原）|
| Docker | 只读容器列表（10 行 + 过滤器 + 分页） | ✓ 无破坏性动作 |
| Database | SQL 控制台默认**禁用**（`[features].unsafe_sql_admin=false`），错误提示可见；写入需 `/* ADMIN_CONFIRMED */` 前缀 | ✓ 策略正确 |
| Terminal | xterm 真实连到 API 容器 shell（`POST /api/admin/terminal/session → 204`，提示符 `…:/var/lib/floatctf/runtime$`） | ✓ |
| Player WriteUp | 提交入口存在，但需**文件上传**（picker 在 Windows 侧） | ⚠ 无法自动化，同 zip/patch 限制 |

**F42（已修）**：通用表格组件的增/改/删成功提示是英文（`Create Users successfully` / `Update Settings successfully` / `Delete … successfully`）→ 改为「创建成功 / 已保存修改 / 删除成功」，浏览器复验 ✓。

**F43（不成立）**：一度认为 SQL 控制台 404（路由缺失），实测为**配置禁用**（返回明确提示），且前端**确实渲染**该提示（此前读取时机过早）；仅文案英文，已中文化（含成功提示与「未知错误」）。

## 九、F24/F26/F44 深挖与反复测试（第 7 轮）

### F24 结论：不是缺陷（不改）

`dynamic_ip_range("10.42.2.0/23")` = `10.42.3.0/24`；父网段是 **/23**，故 `10.42.3.0` 在 L3 上是**合法主机地址**：宿主实测 `http://10.42.3.0/` → **200**（确有服务在监听）。固定网关由平台显式放在池外（`10.42.2.128`），不会与池内地址冲突。唯一冲突场景（不指定网关时 Docker 把 IPRange 首地址当网关，导致 `.0` 不可分配）已用一次性网络复现，但不适用于 AWDP 网络。真实故障是**跨 docker 网络隔离** → 见 F26。

### F26 + F44：已修复并部署（配置开关控制）

**F26**：manual Test Check 在生产必然失败（API 容器只在控制网，脚本却在 API 进程内执行）。
**F44（排查中发现的更严重问题）**：API 的进程内 worker 会与 JudgeServer **争抢 official 作业**，其注释假设「进程内 worker 在宿主网络」——该假设仅 dev 成立，生产上被它领到的评测同样失效。

**修复**：`exec_in_data_plane`（经 helper `docker exec`，stdin 传脚本）把 health/judge/exploit 送进**该赛事的 JudgeServer 容器**执行；manual 与 official 两条路径统一走「优先数据面、容器不可用回落进程内」。由动态设置 `AWDP_DATA_PLANE_EXEC` 控制（默认关闭=既有行为，生产已置 true）。

**验证**：关闭时与此前基线**完全一致**（`awdp_fix_patch` 3 例 + `awdp_concurrency` 4 例 = **7/7 通过**）；开启后生产 manual Test Check 的探针**确实在练习判题容器内执行**并返回真实数据面结果（判题容器 → 靶机 `10.42.3.1:80` 超时，而非 API 侧连接错误）。提交 `64563d5`。

### 反复测试（10 连测）

- 第一次 10 轮：第 1 轮 PASS；**第 2–4 轮因我在 soak 运行期间编辑源码而编译失败**（流程教训：连测期间冻结源码）；**第 5–10 轮 `awdp_concurrency` 持续失败——由我的数据面改动引入的真实回归**（python 探针在测试环境超时）→ 改为「本地探针优先 + 开关」后修复。失败级联使残留涨到 197 容器/197 网络（已清理）。
- 第二次（干净，全程冻结源码）10 连测：**10/10 PASS** ✓；单轮 118–165s（首轮含编译），均值约 130s，**零失败**。
- 浏览器 E2E 循环 10 次（练习赛：启动实例 → 错误 flag → 正确 flag）：**10/10 通过** ✓，平均 **11.8s/次**；10 次错误 flag 全部返回「提交失败：flag 错误」，10 次正确 flag 全部返回「提交成功，该题已解出」。
- 清理后再跑一次全量回归仍 **PASS** ✓（确认失败与残留无关）。
- **已量化测试卫生问题**：每轮全量回归泄漏**约 29 个容器 + 18 个网络**（10 轮累计 67→327 容器、40→202 网络）。构成：`fctf-awdp-judge-*`（awdp-judge）135 个、`awdp-*`（awdp-instance）85 个、`fctf-awdp-*` 网络 137 个 → 即每轮约 14–15 个 AWDP 真实宿主用例各泄漏「判题容器 + 实例容器 + 网络」。建议给这些用例补 teardown（或在 `test-rust.sh` 收尾按「非生产事件标签」清理）。本轮共清理 **314 容器 + 193 网络**。

## 十、边界（boundary）专项测试（第 8 轮）

方法：逐条枚举校验规则，按 min-1 / min / min+1 / max-1 / max / max+1、空值、超长、Unicode、端点、非法类型取值扫描；发现的缺陷直接修复并部署验证。

### 结果矩阵（摘要）

| 边界族 | 用例数 | 结果 |
|---|---|---|
| AWD 配置 10 个数值字段 × 6 取值 | 60 | **全部符合预期** ✓（端点内接受、端点外 400 中文提示，无 off-by-one）|
| AWDP 配置（fix 时长/回合间隔/分数）| 24 | 取整向下（300/7→294）为 V2 设计 ✓；分数无上限 → **F48 已修** |
| 用户名/密码/邮箱 | 9 | 64/65 字符、8/7 位边界正确 ✓；唯一冲突文案 → **F45 已修** |
| 赛事标题 | 6 | 空/纯空白 400 ✓；1/200/5000 字符与 Unicode/emoji 均接受（**无上限**，见下）|
| 时间 | 3 | start==end、start>end 拒绝 ✓；过去时间可建（管理端回溯建赛，属预期）|
| 分页 | 8 | `limit` 生效、超页被夹到最后页 ✓；非法参数 → **F46 已修** |
| 权限/状态 | 10 | 守卫全部正确 ✓（401/404/400）；但文案多为英文（见下）|
| 平台网络设置 | 19 | 前缀 15–24 有范围保护 ✓；派生字段写入被忽略 ✓；公共端点 → **F47 已修** |

### 本轮修复（均已部署 + 生产验证）

- **F45** 唯一冲突文案固定「用户名已被占用」：`users` 同时有 username/nickname 唯一约束，实测仅昵称重复也报用户名 → 按约束名精确提示（生产实测「昵称已被占用」）✓
- **F46** 提取器错误未走平台信封：非法查询参数返回纯文本英文 `Query deserialize error: …`、畸形 JSON `Json deserialize error: …`、错误 Content-Type `Content type error`、超大数字空体 400 → 统一为 `{code,message,data,meta}` 中文提示 ✓
- **F47** `wireguard_public_endpoint` 接受任意字符串（实测 "not-a-url" 落库并会下发客户端）→ 新增格式校验（IP 或含点域名 + 可选 1..=65535 端口，含单测）✓
- **F48** AWDP `fix_round_score`/`break_score` 无上限（i64::MAX 可存，派生 break_score 在启动物化时溢出）→ 加 MAX_SCORE=1e9 上限 + 派生改饱和运算 ✓

### 补充（第 8 轮续）

- **flag 提交边界（真实浏览器）**：空字符串 / 纯空白 / 错误 flag 均被**干净拒绝**并提示「提交失败：flag 错误」✓；**尾部空白会被 trim 后判对**（提交成功）✓；成功后实例与输入框同时消失（状态边界符合预期）✓；无 500/异常。
- **AWDP 配置校验文案中文化引发的测试断言修复**：`awdp_domain` 4 例因断言英文子串（`"> 0"`、`"at least"`、`">= 0"`）失败 → 我把断言改为匹配中文；同时补译 `break_duration_secs must be at least {}s …`、`expected_updated_at is required for config update`。修复后 `awdp_domain` **6/6 通过** ✓。
- 已用脚本全量核对测试中的英文断言是否仍存在于源码（7 处疑点经人工核对均为测试本地字符串或 PG 约束消息，无需修改）✓。

### F49 分页端点边界（本轮修复）

浏览器实测：在第 1 页点「上一页」→ 查询页码变成 0 → **列表整页变空**（Primer 控件 `aria-disabled=true` 仍会回调 `pageIndex=0`）。后端 `paginate_query` 对 `page==0` 直接返回空集，却对 `page>总页数` 夹到末页 → 语义不一致。

修复：后端把 `page==0` 按第 1 页处理（`limit==0` 仍返回空）；前端 `setPage(Math.max(1, pageIndex + 1))` 兜底。

验证（生产）：`page=0/1/2/99999` → 10/10/2/2 条（0 与 1 一致、超页夹末页）✓；浏览器复验第 1 页点「上一页」后仍为 **10 行**（此前变空）✓。

### 尚未处理的边界（建议）

1. **英文错误文案面**：权限/状态守卫返回英文，例如未认证 401「Invalid or missing token…」、非法 UUID 404「UUID parsing failed: …」、AWD 启动「Cannot start event in Configuring status.」、暂停「Can only pause a running event」；删除不存在事件返回 **空消息 404**。建议按模块批量中文化。
2. **超长文本无上限**：赛事标题/描述 5000 字符可入库（实测），可能撑坏列表与详情布局，建议加长度上限。
3. **分页 `page=0` 返回空集**（`page≥末页` 会夹到最后页），语义不一致，建议统一为「<1 视为 1」。
4. 题目标题/flag 的创建与提交边界本轮因请求体形状未对齐未重测（早前轮次已覆盖 flag 语义边界）。

## 八、运行态与门禁

- 生产：API `floatctf/api:0.3.3` healthy；前端 `index-Bcw-Nr6T.js`；每次部署都有 `*.bak-*` 备份。
- 门禁：全量 Rust `scripts/test-rust.sh` PASS（期间抓到 1 例我引入的回归：练习防御误伤自建练习赛 → 已修 `1c3f4bd`）；前端 **218 用例 PASS**、`tsc`、`biome lint`、`vite build` 全绿。
- 数据：35 个用户（含 30 个 `qa01`–`qa30` 压测用户）、13 道题（12 ready）、4 个赛事（2 练习 + 3 压测/历史）、1 个 GameBox；测试残留容器与网络已清零。
