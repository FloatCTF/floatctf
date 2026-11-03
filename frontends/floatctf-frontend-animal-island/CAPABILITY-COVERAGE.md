# 能力覆盖状态（CAPABILITY-COVERAGE）

> **这是诚实清单，不是宣传页。** 逐行对照
> [CAPABILITY-MATRIX.md](../../docs/frontend/CAPABILITY-MATRIX.md) 的 `required` 能力。
>
> 状态口径：
> - **完成** = 接了真实 API、覆盖 loading/empty/error 三态、按设计法渲染；
> - **骨架** = 路由存在、页面渲染占位卡，**未接真实 API**；
> - 本清单描述的是**当前工作树**（尚未提交）。

## 平台引导与运行时契约

| 能力 | 状态 | 位置 |
|---|---|---|
| `mount(context)` 契约、清理函数 | **完成** | `src/app/mountApp.tsx`、`src/entry.tsx` |
| 用 `apiBaseUrl` 建 SDK 客户端 | **完成** | `src/api/client.ts` |
| 制品 manifest + 构建期自检 | **完成** | `vite.config.ts`、`floatctf.frontend.json` |
| 契约版本一致性 | **完成** | `apiContract: "2"` |
| SSE 传输原语 | **完成** | `src/api/react.ts` —— 经 `@floatctf/react` 的 `useAwdEventStream` / `useAdminAwdEventStream` / `useAwdpEventStream` / `useAwdpRunStream`（其内部调用 `client.sse.connect*`） |

## 认证与会话

| 能力 | 状态 | 位置 |
|---|---|---|
| 登录（`POST /api/session`） | **完成** | `src/features/auth/Login.tsx` |
| 会话持久化（localStorage，版本化迁移） | **完成** | `src/auth/store.ts` |
| `/me` 引导、权限读取 | **完成** | `src/app/mountApp.tsx` |
| 401 / 403 分流（401 清会话，403 **绝不清**） | **完成** | `src/api/client.ts` |
| 路由守卫 + 深链 `next` 回跳（含开放重定向防护） | **完成** | `src/app/App.tsx` |
| 改密 / 改资料 | **完成** | `src/features/island/Profile.tsx`（改密会明示吊销其它会话；展示有效权限） |
| 忘记密码（重置申请） | **完成** | `src/features/auth/Login.tsx` —— 以用户名**或**邮箱申请；不论账号是否存在都返回同样结果（避免变成账号枚举接口） |

## 选手端

| 能力 | 状态 | 位置 |
|---|---|---|
| 总览（我的赛事 / 待办） | **完成** | `island/Home.tsx` —— 按进行中/即将开始/已结束分组，接 `events.fetch()` |
| Top15 解题榜 | **完成** | `island/Home.tsx` —— `solves.getTop15Users()`，全平台解出题数排行 |
| 赛事列表 | **完成** | `island/EventList.tsx` —— 赛制+状态双筛选，卡片网格 |
| 赛事详情 | **完成** | `island/EventDetail.tsx` —— 加入/退出、四分区标签页（题目/公告/实例/队伍）、战队创建与退出、赛事内提交（**先查已有实例再决定是否启动**） |
| 题目列表（练习） | **完成** | `island/ChallengeList.tsx` —— 分类+关键词筛选、解出状态 |
| 题目详情 / 提交 | **完成** | `island/ChallengeDetail.tsx` —— 启动实例、展示连接信息、提交 flag |
| 排行 + 走势 | **完成** | `island/Scoreboard.tsx` —— 赛事选择、**动态题目列**由计分榜首行推导；走势以时序表呈现（不引图表库、不自绘 SVG） |
| 我的资料 / 改密 | **完成** | `island/Profile.tsx` —— 改资料+改密（明示会吊销其它会话）、展示有效权限 |
| 讨论区（列表 / 详情 / 评论 / 点赞） | **完成** | `island/DiscussionList.tsx`（搜索+发帖+点赞）、`island/DiscussionDetail.tsx`（评论增删改、点赞、删除讨论需输入确认词） |
| Writeup（我的 / 公开 / 提交） | **完成** | `island/WriteupList.tsx` —— 公开题解（Drawer 阅读）+ 我的题解（只能对**已解出**的题撰写） |
| **AWD** 选手端 | **完成** | `island/AwdEvent.tsx`（`/island/awd/:eventId`）—— 战况 / 提交 flag / 我的靶机（含重置）/ 积分榜 / 接入方式（WireGuard + SSH）。**实时**：`useAwdEventStream`，断流时显式显示「轮询兜底」 |
| **AWDP** 选手工作台（比赛） | **完成** | `island/AwdpEvent.tsx`（`/island/awdp/:eventId`）—— 按**阶段**组织（Break 攻破 / Fix 修复）：实例生命周期、源码下载、flag 提交、补丁上传、手工自检、回合 / 评测 / 积分榜（矩阵明细）/ 趋势。**实时**：`useAwdpEventStream` |
| **AWDP** 练习目录 | **完成** | `island/AwdpPractice.tsx`（`/island/awdp/practice`）—— 箱子目录 + 「继续进行中的练习」优先引导（避免制造孤儿 run） |
| **AWDP** 练习工作台 | **完成** | `island/AwdpRun.tsx`（`/island/awdp/runs/:runId`）—— 开始/停止/重置/重开/结束、自由切阶段、补丁、手工自检、**全量校验**、回合/评测/积分流水/题解。**实时**：`useAwdpRunStream` |

## 管理端

| 能力 | 状态 | 位置 |
|---|---|---|
| 控制台总览 | **完成** | `console/Home.tsx` —— 需要关注（失败任务/错误日志/AWD 告警）→ 平台规模 → 赛事状态 → 最近动静 |
| 赛事管理 + 详情 | **完成** | `console/Events.tsx`（CRUD；删除需输入标题确认；编辑时禁改赛制）、`EventDetail.tsx`（规模/Top10/解出分布/流水 + 报告与 Writeup 导出 + **参与用户**（增删/封禁/解封）+ **赛事公告** CRUD + **统一实例列表**） |
| 题库 / 题集管理 | **完成** | `console/Challenges.tsx` —— 导入→校验→构建→扫描全生命周期 + 题集 CRUD |
| 箱子库 | **完成** | `console/Gameboxes.tsx` —— 导入/扫描/校验（全量带强确认）/编辑/隐藏（非删除）/删除 |
| 武器库 | **完成** | `console/Weapons.tsx` —— CRUD + 附件上传（缺文件时强调上传按钮） |
| 靶场网络（平台级） | **完成** | `console/Network.tsx` —— 网段池设置（显式编辑+确认词）、健康检查、网段分配表 |
| 公告 / 讨论治理 | **完成** | `console/Announcements.tsx`（含「账号已注销」快照标记）、`console/Discussions.tsx`（删讨论/删评论；**不假装提供"编辑他人内容"**） |
| 用户 / 用户详情 / 角色与权限 | **完成** | `console/Users.tsx`（CRUD，密码只写不读）、`UserDetail.tsx`（**整体替换**式角色授予 + 反提权 403 如实展示 + 权限预览）、`Roles.tsx`（权限来自后端目录；内置角色权限集禁改） |
| 审计日志 | **完成** | `console/Logs.tsx` —— 级别/分类/主体筛选；无主体的日志显示为「系统」而非空白 |
| 动态设置 | **完成** | `console/Settings.tsx` —— `value` 与 `resolved_value` 双列（否则"改了没生效"查不出原因）；受保护项禁改 |
| 前端管理 | **完成** | `console/Frontends.tsx` —— 同源注册表 + `parseRegistry` 权威校验；激活写 `FRONTEND_ACTIVE`；标注 `?frontend=default` 应急入口 |
| 计划任务 | **完成** | `console/Tasks.tsx` —— CRUD + 立即执行（强确认）；受保护任务禁删 |
| 版本与系统信息 | **完成** | `console/Version.tsx` —— 版本/CPU/内存/磁盘（≥90% 标红）/网卡/Docker 镜像 |
| 容器运维 | **完成** | `console/Docker.tsx` —— 容器/镜像/网络三分区；三类删除均需确认词；运行态从 `status` 文本派生（DTO 无 `state`） |
| SQL 控制台 | **完成** | `console/Database.tsx` —— 非查询语句需先解锁开关；执行前回显 SQL；历史保留 20 条并可对比 |
| Web 终端 | **完成** | `console/Terminal.tsx` —— xterm + WebSocket；**凭据经由 60 秒单次 HttpOnly 票据过桥，URL 上无任何凭据** |
| **AWD** 管理控制台 | **完成** | `console/AwdEvent.tsx`（`/console/awd/:eventId`）—— 生命周期（部署 → 开始 ⇄ 暂停 → 结束 → 归档）/ 参数配置（乐观锁）/ 预检与凭据轮换 / 队伍封禁解封与调分 / 靶机挂载 / 赛事网络 / 积分榜。**实时**：`useAdminAwdEventStream` |
| **AWDP** 管理控制台 | **完成** | `console/AwdpEvent.tsx`（`/console/awdp/:eventId`）—— 阶段推进（开始/转入 Fix/结束）、参数配置（乐观锁）、题目挂载（用 AWDP 目录判 `awdp_capable`）、实例、积分榜、总览数据 |

## 结论

**覆盖口径：`complete（选手端 + 管理端）`。**

### 判定依据（全部可复核，不是自述）

| 项 | 证据 |
|---|---|
| 能力覆盖 | `CAPABILITY-MATRIX.md` 的 **93 条 `required` 逐条机械核对**，见下文 |
| 页面状态 | **37 个页面，0 个骨架页**（`grep -l 占位实现` 为空） |
| 类型 | `pnpm exec tsc --noEmit` → **0 错误** |
| 构建 | `vite build` → 成功 |
| 制品 manifest | `parseFrontendManifest(dist/frontend.json)` → **通过**（`id=animal-island` `v0.1.0` `apiContract=2`，无 `build` 段） |
| 源码 manifest | 平台自带 `scripts/frontend.sh` 的 `validate_source_manifest` → **通过** |
| 入口/样式存在 | `dist/assets/frontend.js`、`dist/assets/frontend.css` 均真实存在 |
| 权限声明 | `nav.ts` 的 **17** 条控制台权限逐条对照 `permission.rs` → **全部存在，无拼写漂移** |
| 设计法 | 机械扫描 Unicode 符号 / emoji / 手写 SVG / 其它图标库 / 纯黑 / 字重 < 400 / token 短名 → **0 违规**（过程修掉 9 处） |
| 假数据 | `src/` 内无 mock / TODO / 占位数据；所有数据面来自真实 SDK 调用 |
| 原生弹窗 | 无 `alert` / `prompt` / `window.confirm`；全部走 `useConfirm` |
| Token 泄露 | 无任何一处把 token 放进 URL / query string |

### 能力核对明细

从矩阵抽出全部 `required` 行 → 提取公共面的 **84 个 SDK 符号** → 在 `src/` 里逐个查找：

```
第一轮：9 个未覆盖  →  补齐 7 项  →  第二轮：2 个
```

补齐的 7 项：忘记密码 / Top15 解题榜 / 赛事题目实例查询 / 赛事维度提交 /
赛事用户管理 / 赛事公告 CRUD / 赛事统一实例列表。

最后 2 个（`client.sse.connectAdmin`、`client.sse.createParser`）**不是缺口**：
本前端使用平台自己的 React 绑定 `useAdminAwdEventStream` / `useAwdpEventStream`，
其内部调用 `client.sse.connectAdmin(...)` / `client.sse.connect(...)`
（已核对 `packages/react/src/useAdminAwdEventStream.ts:135`）。
React 前端通过平台提供的绑定使用传输原语，正是设计意图。

### 本会话**未**完成的验收项（如实说明）

[AGENTS.md](./AGENTS.md) 的 DoD 含一条：

> 在 FloatCTF 实例上完成安装与验收（`frontend.sh install` → 激活 → 刷新 →
> 真实数据 → `?frontend=default` 回退）

**该条未执行**，因为本次任务的约束明确禁止启动 dev API
（`HANDOFF.md` 记录：启动 dev API 会在约 8 秒内删除并重建生产
`fctf-awdp-practice-judge`）。因此：

- 上述所有验证都是**静态 + 构建期**的（类型、构建、制品、manifest、权限串、设计法扫描）；
- **运行时端到端**（真实登录 → 真实数据 → 深链刷新 → 回退）**尚未在实例上验证过**。

这不影响能力覆盖的判定（覆盖由"是否接了真实 API"决定，已逐条核对），
但它是交付前仍应补上的一步，**需要在一个安全的实例上进行**。

---

## 已确认的 API 事实（免得下一轮重新摸）

- `EventInfo.id` **运行时不返回** → 一律用 `~/lib/event.ts` 的 `eventIdOf(info)`。
- 赛事**没有 status 字段**，状态由 `start_time`/`end_time` 推导（`eventStatus()`）。
- **`InstancesDto.flag` 是答案本身**，选手端任何页面都不得渲染它。
- 后端按 `instance_id` 计分 → 提交前必须先拿到实例；赛事维度提交用 `submitSingle`
  （带上 `event_id`），而不是 `submit`。
- `EventTeamMemberResult` 是 `{ member_name, member }`，成员主键是 `member.user_id`（复合键，无 `id`）。
- 排行榜每行自带 `challenges[]` → 表头由首行动态推导，不硬编码题目列表。
- **`DiscussionComments` 只有 `author_id`，没有作者昵称**；它是生成实体，只从
  `@floatctf/sdk/entity` 导出（**不在**根导出面里）。页面运行时试探可能的昵称字段，
  拿不到就退化成短 id + tooltip，而不是显示假名字。
- `createMyWriteup` 收的是**文本** `{challenge_id, content}`，不是文件上传。
- 写题解只对**已解出**的题开放（`challenges.fetch()` 的 `solved` 字段）。
- **箱子 / 网络 / AWD / AWDP 的 API 在 `client.awd.admin` 与 `client.awdp.admin`**，
  **不在** `client.admin.*` 里（`client.admin` 下没有 gamebox 模块）。
- `ImportChallengeResponse` 是 `{ challenge }`；`ChallengeCheckResult` 用
  `challenge_name` + `is_ok` + 三个布尔，`BuildChallengeResult` 用
  `challenge_name` + `message` + `is_ok` —— 两者形状**不同**，不要互抄。
- `ChallengeSets` 实体**没有** `challenge_count` 字段。
- `Announcements.publisher_id` 可为空且删除账号时置 NULL，但 `publisher` 文本保留 ——
  这正是「发布者账号已注销」的判据。
- 平台级网络设置是**宿主级**配置：表单默认只读、显式进入编辑态、保存需确认词。
- `Checkbox` 是**分组**组件（`value: (string|number)[]` + `options`），**不是**单个勾选框。
- `ScheduledTasks.trigger_type` / `.status` 是**普通字符串列**（无 PG enum）。
- `SystemInformation` 的真实字段是 `nb_cpu` / `disks_info` / `docker_info` / `host_name`。
- `FloatDockerContainer` **没有** `state` / `names`，只有扁平的 `name` + `status` 文本；
  带 `state` 的 `ContainerInfo` 存在但**不是** `fetchContainers` 的返回类型。
- `GameBoxLibraryDto` **没有** `awdp_capable`；该标记在 `GameBoxCatalogDto`
  （`awdp.runs.gameboxCatalog()`）上。挂载下拉必须用后者，否则会挂上不支持的箱子。
- `EventAnnouncements` / `Announcements` / `Logs` / `ScheduledTasks` / `Settings` /
  `DiscussionComments` 从 `@floatctf/sdk/entity` 导入；
  `PermissionDto` / `RoleDto` / `UserRolesDto` / `SettingsDto` / `EventUserResult` /
  `AdminInstanceRow` 从根导出。
- `TeamMemberResult` = `{ username, nickname, role, points }`（与 `EventTeamMemberResult` 不同）。
- 后端 `adjustScore` 的 body 是 **snake_case**（`team_id`）。

## 实时流（SSE）

**AWD 事件类型**（逐条取自后端 `AwdEvent::new(...)` 的全量调用点，未猜测）：
`round.started` / `round.completed` / `round.ended` / `attack.success` / `judge.result` /
`score.changed` / `gamebox.reset` / `team.banned` / `team.unbanned` / `event.finished` /
`event.paused` / `event.resumed` / `network.policy.applied` / `network.policy.failed` /
`network.error`，以及传输层的 `stream.lagged`。

**query key 与 `@floatctf/react` 的失效列表逐字对齐**：

- 选手端：`awd-player-status` / `awd-gameboxes` / `awd-scores` / `awd-wg` / `awd-ssh`
- 管理端：`admin-awd-status` / `admin-awd-scores` / `admin-awd-prechecks` / `awd-gameboxes`

这不是风格问题 —— SSE 连上后**轮询回退会停止**（两者互斥），失效列表一旦比页面实际读的
key 窄，面板就会在整场比赛里悄无声息地停更，而连接状态看起来完全正常。
因此两个 AWD 页面的注释里都写明了「不要自己发明 key 名」。

## 机械校验：nav 权限串

`~/app/nav.ts` 声明的 **17** 条控制台权限已对照
`apps/api/src/core/permission.rs` 的权限字符串逐条核验：**全部存在，无拼写漂移**。

（这项检查值得做成常设的 —— 一个拼错的权限串不会报错，只会让入口**永远不显示**。）

AWD / AWDP 的新页面没有新增导航项：它们的入口放在赛事详情页
（`family === "awd"` / `"awdp"` 时各出现一个按钮），因为这两个赛制的一切都以**赛事**为作用域，
塞一个需要 `eventId` 的顶层导航项没有意义。AWDP 练习目录是例外（它不绑定赛事），因此有独立导航项。

## PUBLIC SDK GAP（不修改上游，仅记录）

1. **前端注册表 / 引导元数据未纳入 SDK**：`GET /__floatctf/frontends/registry.json`
   （只读静态文件，无鉴权）与 `GET /api/frontend`（引导元数据）在 SDK 里都没有对应方法
   （`client.admin` 下不存在 frontend 模块）。
   `console/Frontends.tsx` 的处理方式是走 AI-FRONTEND-GUIDE 允许的**同源静态资源逃逸口**：
   裸 `fetch` 读同源注册表 + 复用平台自己的权威校验器 `parseRegistry`，
   保证「前端看到的注册表」与「平台实际加载的注册表」是同一份、同一套规则校验。
   激活本身**有** SDK 路径（写 `FRONTEND_ACTIVE` 动态设置）。
2. **讨论评论不返回作者昵称**：`DiscussionComments` 只有 `author_id`。
3. **Web 终端未纳入 SDK**：`POST /api/admin/terminal/session` 与
   `GET /api/admin/terminal/ws` 在 SDK 里都没有对应方法。
   这不是可以绕过的缺口，而是**必须**裸写的地方：浏览器不允许给 WebSocket
   设置自定义头，因此令牌只能靠 cookie 过桥。
   平台的两步握手正好为此设计 —— 先用 `Authorization` 头换 **60 秒、单次使用、
   HttpOnly、SameSite=Strict、path 限定为 `/api/admin/terminal/ws`** 的票据 cookie，
   再用它升级 WS。**实现里没有任何一处把令牌放进 URL/query string。**
   注意升级前后端会**重新解析权限并现查 `terminal.open`**，因此撤销权限后旧票据也无效。

## ✅ 平台侧安全缺陷：已修复（含第 2 处）

实现「赛事用户管理」时发现，并已**修好**（用户明确授权改动后端）。

### 缺陷

```
GET /api/admin/events/{event_id}/users          （仅需 events.read）
  → EventUserResult { user: users::Model }      ← 生成的 DB 列镜像，含 password
  → users::Model 派生 Serialize，password 无 #[serde(skip_serializing)]
  → 响应 JSON 带每个参与者的 argon2id 哈希
```

**根因不是"某个人写错了一行"，而是三层叠加：**

1. **实体层**：`sea-orm-codegen` 给每个生成的 `Model` 派生 `Serialize`，
   且**没有**列级跳过机制 —— 而实体是生成物、铁律禁止手改。
   因此 `users::Model` 一旦被序列化，`password` 就必然跟着出去。
2. **约定层**：平台的安全不变量 #1 要求「响应 DTO 独立定义、永不复用生成实体」
   （`identity/user/dto.rs` 文件头写得很清楚，且记录了历史上修过同款问题）。
3. **守卫层（真正的漏点）**：那条约定**只在 `identity` 模块内被测试强制**——
   `identity_wave0_security.rs::identity_dtos_declare_no_password_field`
   只扫 `apps/api/src/modules/identity`，且强断言只作用于 `dto.rs`。
   于是**不变式的作用域是全平台，守卫的作用域却是一个模块**，
   范围外的违反不会有任何测试发现。

### 修了什么

| # | 位置 | 改动 |
|---|---|---|
| 1 | `event/common/api/event_users.rs` | `EventUserResult.user`: `users::Model` → **`identity::user::dto::UsersDto`**（全平台唯一的"账号出现在响应里"的表示）；并补上行标识 `id`（`event_users` 是复合主键，SDK 早已声明该字段但后端从未返回） |
| 2 | `event/common/api/event_teams.rs` | `GetTeamMembersResult.members`: `Vec<users::Model>` → **`Vec<UsersDto>`** |

第 2 处是**新守卫自己发现的**：写完守卫跑第一次就红了，报出
`event_teams.rs:245: pub members: Vec<users::Model>` —— 同一个 bug、另一个端点
（`GET /api/admin/events/{id}/teams/{team_id}/members`，同样只要 `events.read`，
全仓无调用方）。

两处都去掉了多余的 `Deserialize`：它们是**响应**类型，无人反序列化
（多派生一个只会扩大使用面，且 `UsersDto` 本就只派生 `Serialize`）。

### 顺手修掉的类型谎言

- SDK `EventUserResult.user`: `Users` → **`UserDto`**（生成实体含 password，
  把它当响应类型等于邀请调用方去读哈希）。
- SDK `event_users.banned/unbanned` 声明返回 `EventUserResult`，
  但后端返回 `()` —— 改为 `UniResponse<null>`。
- `frontends/floatctf-frontend-shadcn/src/features/admin/events/tab-members.tsx`
  把 `displayName(user: Users)` 收窄为 `UserDto`，并把平台账号列表的
  `callList<Users>` / `DataTableColumn<Users>[]` 改为 `UserDto`
  （`client.admin.users.fetch` 返回的本来就是 `UserDto[]`）。

### 新增的两道守卫（这才是防复发的关键）

| 测试 | 作用 |
|---|---|
| `no_serialized_response_struct_holds_a_sensitive_entity_model` | **扫描整个 `apps/api/src`**（排除 `entity/`）：任何 `derive(Serialize)` 的结构体都不得持有 `users::Model` / `awdp_evaluation_proofs::Model` / `awd_wireguard_peers::Model`。用**标识符边界**匹配，避免 `event_users::Model` 误报成 `users::Model` |
| `event_user_result_serializes_without_password` | 运行时断言：用**真带 argon2 哈希**的实体构造响应，序列化后不得出现 `password` / `$argon2` / `token_version`，且必要字段仍在 |

守卫的**作用域**现在是全仓，与不变式的范围一致 —— 这正是原来缺的那一环。

### 验证

```
cargo test -p floatctf --test identity_wave0_security ...... 7 passed
cargo test -p floatctf --test event_domain_invariants ...... 15 passed
cargo check -p floatctf --tests ............................ 0 errors
mise run web:typecheck ..................................... exit 0（6 个包）
xzmu 前端 tsc --noEmit ..................................... exit 0
mise run web:architecture .................................. OK
cargo fmt --all --check .................................... 干净
```

**仍未做**：这两个端点的运行时端到端验证（需在安全实例上跑，本次约束禁止起 dev API）。
守卫与序列化测试是静态 + 单元级的证明。

## 一处声明式偏离

机器标识符（UUID / IP / CIDR / 容器名 / 配置键值 / SQL 结果）使用等宽字体，共 25 处。
设计法的字面例外只有 `CodeBlock` 与终端输出。保留理由是**平台既有约定**：
Default Frontend 对同类数据同样使用 `font-mono`
（`EventInstancesTable.tsx:36`、`FrontendSelector.tsx:253`、`AwdpWorkbench.tsx`）。
判定过程与完整扫描结果见 [FRONTEND-PLAN.md](./FRONTEND-PLAN.md) §9。
