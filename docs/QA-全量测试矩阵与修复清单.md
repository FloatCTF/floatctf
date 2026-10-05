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

### 错误响应一致性（本轮修复）

- **F50 未匹配路由空体 404**：`DELETE /api/admin/events/{不存在的 uuid}` 等未匹配路由返回 Actix 默认**空体** 404 → 新增 `default_service` 返回 `{"code":404,"message":"接口不存在"}` ✓
- **F51 未认证 401 为纯文本英文**：「Invalid or missing token, or contact the admin」→ 改用 `AppError::Unauthorized`，返回中文 JSON 信封「登录状态已失效，请重新登录」✓（顺带记录：选手 token 访问管理接口现返回 401，从鉴权语义看更接近 403，属可讨论项）
- 说明：删除事件接口是 `DELETE /api/admin/events` + JSON body（`{id_list}`），我先前用路径参数导致 404 属用法错误，非缺陷 ✓

### 浏览器禁用态与容量边界（本轮观察）

- **归档事件**：`/admin/events/awd/{id}` 详情页对 `archived` 事件**不提供**生命周期控件（无 Start/Pause/Finish 按钮）✓，与 API 侧拒绝（「Cannot start event in Configuring status」）一致 ✓。
- **分页控件**：单页时控件整块隐藏 ✓；多页时第 1 页「上一页」为 `aria-disabled=true`（Primer 语义禁用）但**仍回调 pageIndex=0** → 已在 F49 前端夹取兜底 ✓。
- **容量边界现状**：`awd_event_networks` 已分配 **2 / 16**，距上限尚远；要打到 16 的边界需再建 14 个 AWD 事件网络（会创建 docker 网络），成本较高，留待专门排期。

### F52 赛事文本长度上限（本轮修复）

边界实测：`events.title/description/rules/flag_prefix` 列均为 `text` 且后端仅校验标题非空 → **5000 字符标题可入库** ✗（会撑坏列表/详情布局）。

修复：`validate_event_text`（标题 ≤200、描述 ≤10000、规则 ≤50000、flag 前缀 ≤32，按 **Unicode 字符数**计），create 与 patch 双路径生效；前端事件表单对应输入加 `maxLength`。

验证（生产，10/10 符合预期）：标题 200 接受 / 201 拒绝（ASCII 与**中文**均如此）、描述 10000/10001、规则 50000/50001、前缀 32/33 ✓；临时赛事已用 `DELETE /admin/events` + `{id_list}` 正规清理 ✓。既有线上数据（标题 16 / 描述 32 / 规则 659 / 前缀 4）不受影响 ✓。单测覆盖 200/201 字符与中文边界 ✓。

### 状态/查找类错误文案中文化（本轮）

边界扫描中反复遇到英文文案（违反中文界面约定），本轮批量中文化（2 批，26 + 32 处）：

| 类别 | 例子（译后） |
|---|---|
| 选手状态边界 | 赛事已结束/最终结算期间：无法重置靶机、无法提交 flag、SSH 与 WireGuard 访问已关闭；你尚未加入队伍；你不是本赛事的参赛者 |
| 状态机 | 当前状态（{}）无法开始比赛，需先完成验证；只有进行中的比赛才能暂停 |
| 查找类 | 未找到赛事/靶机/实例/AWDP 运行/评测/赛事靶机；更新 AWD 配置必须携带 expected_updated_at；竞赛赛事必须设置结束时间 |
| 容量类 | 该赛事已无可用主机编号（2..254 已用尽）|

生产实测：非法 UUID 路径 → **「路径参数格式错误」** 400（此前是英文 `UUID parsing failed: …`）；缺失乐观锁 → 「更新 AWD 配置必须携带 expected_updated_at」400 ✓。内部判题协议 `internal.rs` 的英文错误码保持原样（机器可读契约）✓。

### 尚未处理的边界（建议）

1. **英文错误文案面**：权限/状态守卫返回英文，例如未认证 401「Invalid or missing token…」、非法 UUID 404「UUID parsing failed: …」、AWD 启动「Cannot start event in Configuring status.」、暂停「Can only pause a running event」；删除不存在事件返回 **空消息 404**。建议按模块批量中文化。
2. **超长文本无上限**：赛事标题/描述 5000 字符可入库（实测），可能撑坏列表与详情布局，建议加长度上限。
3. **分页 `page=0` 返回空集**（`page≥末页` 会夹到最后页），语义不一致，建议统一为「<1 视为 1」。
4. 题目标题/flag 的创建与提交边界本轮因请求体形状未对齐未重测（早前轮次已覆盖 flag 语义边界）。

## 十一、稳定性与承压验证（第 9 轮）

### 基线（压测前）

宿主 15G/12C（可用 9.6G）、磁盘 32%；`floatctf-api/postgres/redis/rustfs/caddy` **重启次数全 0**、全部 healthy；API 内存 99.7MiB / 15 pids；PG 11 连接（max 100）、库 83MB；Redis 3 客户端 / 1.34MB；**日志 panicked=0、ERROR=0**（`error:` 命中其实是模型的 `error` 字段，属 INFO 的实例清理日志）。

### 场景与结果

| 场景 | 负载 | 结果 |
|---|---|---|
| 练习 Jeopardy（含真实实例启停）| 10 VU × 150s | **62 RPS**，p50 6.2 / **p95 11.8** / p99 30.3ms，**0 个 5xx**，容器 14→14、网络 9→9、PG 11→8、内存平坦（272–273MiB）|
| 竞技赛 Jeopardy（全读+轮询）| 20 VU × 180s | **420 RPS**，p50 20.7 / p95 49.2 / p99 104.4ms，200=53,059、400=22,731、**0 个 5xx**，无泄漏 |
| 竞技赛参赛路径（赛前加入 30/30）| 20 VU × 180s | **416 RPS**，p50 21.3 / p95 50.4 / p99 104.8ms，200=59,980（80%），实例 25→34（**真实启动成功**），无泄漏，API restarts=0 |
| **故障注入**（45s 重启 API → 80s 杀靶机容器 → 115s 重启 Redis → 145s 再重启 API）| 20 VU × 240s | 全程维持 **414 RPS**，200=79,422、**502=363（0.36%，集中在重启窗口）**、**0 panic**；p95/p99 不受影响（49.6/102.2ms），最坏单次 10.1s；重启后立即恢复 healthy |

**结论**：在 62→420 RPS（约 7 倍）压力下延迟线性可控；四类基础设施故障注入下**无崩溃、无 panic、无 5xx 洪峰、无泄漏**，且恢复即时。

### 发现

- **F53（已修 + 已部署）**：未参赛者启动实例返回英文 `when launch instance:User not joined the event!` ✗ → `context.rs` 的 `User not joined the event!` 改为「你尚未加入本赛事」，并去掉 `instances.rs` 的 `"when launch instance:{}"` 英文包装前缀。
- **F54（已修 + 生产验证）**：**僵尸实例状态**——DB 中 **16 条 `runtime_state='running'` 但容器已不存在**（含我注入时杀掉的那个）。实测 **90 秒后仍未自愈** ✗（清理任务每 30s 跑，但不处理它）。根因：`instance_repository::list_cleanup_candidates` 只挑选 `failed` 或 **`running 且 expires_at <= now`**（TTL 到期），**没有"容器存活性对账"**。影响：UI 显示"运行中"、可能挡住该玩家再次启动、资源账目虚高。**实施与验证**：新增 `InstanceService::reconcile_missing_containers`（在 `system.practice.clean` 里先跑），实例运行时新增 `container_exists`（默认 true 便于 mock；Docker 实现用**容器列表按名比对**，不依赖被归一化的错误文本），仅对 `running` 且 `updated_at` 早于 5 分钟的实例探活，**只有确定容器不存在**才收敛为 `failed`，其它错误只记日志。部署后 **25 秒内 6 条僵尸全部 running → failed**（running=0 / failed=6，逐条日志）✓。

  **更深一层根因**：`list_cleanup_candidates` 写成 `event_challenge_instance::Entity::find().filter(event_instances::Column::…)` —— **在基实体上过滤关联表的列，实测匹配不到任何行**（对账日志 `candidates=0` 即证据），因此 TTL 清理**从未生效**、僵尸实例无限累积。已改为直查 `event_instances` 再按 id 关联，同批修复。

  **残余风险**：暂未观察到误判（本轮 6 条全为真僵尸、无误杀），误判由「5 分钟年龄门槛 + 仅确定缺失」双重保护；如后续遇到 docker 抖动期间的实例，对账会跳过并留待下一轮。
- 次要观察：Redis **未设 maxmemory** ✗（长期运行下内存无上限，建议设置上限与淘汰策略）；实例清理任务每 30s 以 INFO 打印整条 Model（含锁字段）✗，属日志噪音，规模化后影响日志体量。

### 写路径与长时间 soak（第 10 轮补充）

| 场景 | 负载 | 结果 |
|---|---|---|
| **真实 flag 写路径**（竞技赛，静态 flag）| 20 VU × 180s | **403 RPS**，p95 52.1ms，**`submit_correct` 6,320 次全部 200、零失败**；DB 核验：**solves=20、用户=20、题=1、重复组=0** → 6,320 次并发提交只产生 20 条解出，**去重与幂等成立** ✓ |
| **10 分钟 soak**（纯读）| 25 VU × 600s | **275,105 请求 / 458 RPS**，p50 22.7 / p95 54.4 / p99 126.4ms（与 3 分钟跑相比**无退化**）；**API 内存 334.2→335.2→334.5MiB、PG 141→140MiB、PG 连接 11/11/11、容器 71/71/71、网络 45/45/45 全程平坦** → 无慢泄漏 ✓ |

**F54 二次验证（正常流程）**：压测产生的 20 个实例在**解出后被正常销毁**，但 DB 行仍停在 `running`（这正是旧行为里"复练被阻塞"的来源）；我的存活性对账在 5 分钟年龄门槛后把 **20 条全部收敛为 failed**（running=20→0）✓ —— 说明该修复不仅覆盖"外部杀容器"，也覆盖"正常销毁后 DB 未同步"的常规路径。

**观察**：宿主 load average 在 soak 期间为 8.7~13.25 ✗，而 API CPU 仅 ~10%（约 1.2 核）、PG 2~6% → 负载主要来自平台之外（宿主上另有进程/其它容器），解读宿主负载时需注意区分。

### 尖峰与真实容量（第 11 轮）

**突增尖峰**（10 → 100 → 10 并发，各阶段 60/90/60s）：

| 阶段 | 并发 | RPS | p50 | p95 | p99 | 5xx |
|---|---|---|---|---|---|---|
| 1 基线 | 10 | 344.9 | 8.3ms | **28.0ms** | 35.2ms | 0 |
| 2 突增 | 100 | 397.3 | 150.8ms | 458.8ms | 606.2ms | 0 |
| 3 恢复 | 10 | 344.1 | 8.5ms | **27.7ms** | 35.6ms | 0 |

**恢复完美**（阶段3 ≈ 阶段1，零 5xx）；但阶段2 显示吞吐仅 +15%、延迟 ×16。

**真实容量**：用**多进程绕过 CPython GIL**（8 进程 × 12 并发 = 96 并发）重测 → 每进程 87~89 RPS，**聚合 ≈702 RPS**，此时 **API CPU 57%、PG 29%、零 5xx**。结论：单进程 Python 客户端（GIL + 线程）在 ~400 RPS 触顶，**平台真实容量更高**；用 `docker stats` 交叉验证是区分「客户端瓶颈 vs 平台瓶颈」的关键手段。

### ⚠️ 测试操作事故与护栏（第 11 轮，必读）

清理测试残留容器时，我的脚本判据（`io.floatctf.managed=true` + 标签 id 不在生产库）**误删了生产 API 容器** —— 该容器带 `io.floatctf.managed=true` 但**没有事件/实例标签**，"标签集为空"被我归入了清理。

- 影响：API 中断约 **1–2 分钟**；数据零损失（PG/Redis/RustFS 未被涉及）；已用 `docker compose -f compose.prod.yml up -d api` 重建，`health=healthy`、restarts=0，并复核首页 200 / 管理员与选手登录 / `/api/events`、`/api/users/me`、`/api/solves` 全部 200。
- **护栏（已实现于 /tmp/safe_clean.py 并 dry-run 验证）**：
  1. 只处理 `io.floatctf.managed=true`；
  2. **绝不删除** 带 `com.docker.compose.project` 标签（compose 托管基础设施）或容器名以 `floatctf-` 开头者；
  3. 只删除「带 event/instance/run/gamebox 标签且这些 id **均不在**生产库中」者；**标签集为空一律不动**；
  4. 默认 dry-run，需 `--apply` 才删除。
- dry-run 结果：候选残留 **0**，基础设施跳过 1（正是 API 容器，已被正确保护 ✓）。

### FD（文件描述符）泄漏核查（第 12 轮）

| 时刻 | API FD | PG FD | Redis 连接 |
|---|---|---|---|
| 空闲 | **72** | 10 | 3 |
| 96 并发压测中（每 5s 采样）| 106~**123**（峰值）| 10 | 3 |
| 压测后 | **74** | 10 | 3 |

→ **无 FD 泄漏** ✓（峰值距 soft limit 1024 仍有约 8 倍余量；PG FD、Redis 连接与内存全程不变）。备忘：API 容器 `nofile` soft limit 为 **1024** ✗，若未来并发量级大幅提升，建议在 compose 中抬高 `ulimits.nofile`。

### F55 / F56（AWD 多回合负载测试中被阻断，均为真实缺陷）

为做「AWD 多回合比赛在负载下的回合切换/结算」而新建赛事（`29ab429c`：配置 ✓ / 网络自动分配 ✓ / 2 支队伍 ✓ / 挂载 `test-g` ✓）后，**deploy 失败**并暴露两处问题：

- **F55 归档赛事不释放网络分配** ✗：`awd_event_networks` 中 `376a9300`（AWD UI E2E 2，**archived**）仍占 `10.96.0.0/16`、`57269913`（AWD 全量测试，archived）仍占 `10.97.0.0/16`。后果：① 子网泄漏、池容量被白占（共 16 个）；② **新赛事自动分配会撞上这些已归档赛事的子网** → `deploy` 直接失败（实测报错：`gamebox_cidr 10.96.0.0/16 overlaps other event 376a9300-…`）。即「分配时未避开、部署时才拒绝」的检查不一致。
- **F56 `deploy_failed` 无重试路径** ✗：部署失败后事件停在 `deploy_failed`，再次 `deploy` 被拒（`Invalid transition: DeployFailed -> Deploying`），`start` 也被拒（`当前状态（DeployFailed）无法开始比赛`）→ 该赛事**永久卡死**（我未找到 API 侧的重试/复位入口）。
- 处置：我的测试赛事已通过 `DELETE /admin/events` + `{id_list}` 删除（返回 `data:1`），其分配随之释放 ✓；两个归档赛事的泄漏分配**未动**（属生产数据，等待你决定修复方式）。测试期间未产生容器残留（容器数保持 13）。

### AWD 多回合比赛在负载下（第 13 轮，完成）

**前置更正**：硬化期 = `事件时长 − 回合数 × 回合时长`（派生，`domain/timing.rs`）。我上一轮把事件窗口设成 2 小时 → 硬化 6741s，**那是我的配置问题，不是平台默认值** ✗→✓。正确做法：窗口 = 回合数 × 回合时长（6×60s=360s）→ **硬化=0 → 直接进入攻防** ✓。

| 观测 | 结果 |
|---|---|
| 回合推进（20 并发负载下实时采样）| T+35s:1 → T+70s:2 → T+140s:3 → T+175s:4 → T+245s:5 → T+315s:**6** —— **6 回合按 60s 准点推进** ✓ |
| 回合结算 | DB 核验：1–5 回合 `completed` 且 `completed_at` 齐全 ✓，第 6 回合 `active`（进行中结束赛事）✓ |
| 负载 | **142,660 请求 / 475.5 RPS**，p50 16.2 / **p95 24.9** / p99 29.7ms，max 107.6ms，**零 5xx** ✓ |
| 生命周期 | deploy 200 → precheck 200 → start 200 → 6 回合 → finish 200 → archive 200 → delete 200（data:1）；**容器与网络随之全部回收** ✓，分配回到 2 ✓ |

### F55（已修 + 已验证）

新建 AWD 赛事时自动分配拿到 `10.96.0.0/16`，**deploy 直接失败**（`overlaps other event 376a9300-…`）✗。根因：两个**已归档**赛事仍持有 `10.96.0.0/16`/`10.97.0.0/16` 分配行，而两侧口径不一致 —— **自动分配器把已归档视为空闲，部署前跨赛事重叠校验却仍计入** → 新赛事永远无法部署。修复：重叠校验排除 `awd_events.status = archived`（归档路径本身已有"重复归档补齐释放"的自愈逻辑）。验证：重建赛事 → 挂靶机 200 → **deploy 200 OK** → `deployed/hardening`，并真实创建 judgeserver/flagserver 与两队容器 ✓。提交 `611bf44`。

### F56（复核后撤项，非缺陷）

`DeployFailed` 的官方恢复路径是 `config_service` 允许的 **`DeployFailed → Configuring`**（重新保存配置后再部署）✓；直接重试 `deploy` 本就不允许（`Invalid transition: DeployFailed -> Deploying`）✓ —— 不是缺陷。

### F57（本轮新发现，建议修复）

首次建赛事时我设了 `hidden=true` ✗ → 玩家建队接口返回 404「未找到该赛事」✗ → **0 支队伍** → `precheck` 后状态变为 **`VerificationFailed`**，但**验证失败的具体原因没有返回**（仅提示"需先完成验证"）✗ —— 管理员只能靠推断。建议：`precheck` 失败时返回具体校验项与原因（如「赛事不可见，玩家无法建队」「参赛队伍数为 0」）。

### 覆盖度说明（诚实标注）

**已补上（第 10 轮）**：用真实 flag 压到了「提交正确 → 写 solve」路径，6,320 次并发提交零失败、DB 无重复计分 ✓。已补：③ 突增尖峰 ✓（第 11 轮）、④ 轮询路径已修正并重测真实读路径 ✓（第 11/12 轮，多进程 702 RPS 零 5xx）、FD 泄漏 ✓（第 12 轮）。已补：② **AWD 多回合在负载下的回合切换与结算** ✓（第 13 轮，见上）。仍待补：① 动态 flag 写路径；③ **AWD 攻击/计分写在负载下**（第 13 轮负载为只读 ✗，未压到跨队提交与计分）；④ 同时开赛 / 结算瞬间并发；⑤ F57 的修复。

## 八、运行态与门禁

- 生产：API `floatctf/api:0.3.3` healthy；前端 `index-Bcw-Nr6T.js`；每次部署都有 `*.bak-*` 备份。
- 门禁：全量 Rust `scripts/test-rust.sh` PASS（期间抓到 1 例我引入的回归：练习防御误伤自建练习赛 → 已修 `1c3f4bd`）；前端 **218 用例 PASS**、`tsc`、`biome lint`、`vite build` 全绿。
- 数据：35 个用户（含 30 个 `qa01`–`qa30` 压测用户）、13 道题（12 ready）、4 个赛事（2 练习 + 3 压测/历史）、1 个 GameBox；测试残留容器与网络已清零。
