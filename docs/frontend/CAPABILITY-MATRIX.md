# FloatCTF 前端能力矩阵（Capability Matrix）

> 本文是**从仓库源码审计生成**的能力基准，用于「用另一个前端完整替换 FloatCTF」时逐项跟踪覆盖度。
> 相关：[`ARCHITECTURE.md`](./ARCHITECTURE.md)（前端平台架构）、[`DEVELOPING.md`](./DEVELOPING.md)（开发 / 外部仓库流程）、[`ARTIFACT.md`](./ARTIFACT.md)（制品 / manifest / 注册表）、[`AI-FRONTEND-GUIDE.md`](./AI-FRONTEND-GUIDE.md)（从零创建前端的权威手册）。
>
> 依赖边界（硬约束）：新前端只能依赖 `@floatctf/sdk` + `@floatctf/frontend-runtime`（React 才可选 `@floatctf/react`）；**禁止** import `frontends/default/*`、`apps/web/*`、`packages/*/src/*`。Default 源码是**语义参照**，不是 API。
>
> 生成口径（可复现）：
> - 公共 SDK 面：`packages/sdk/src/index.ts`、`client.ts`、`api/**`；`packages/react/src/**`；`packages/frontend-runtime/src/**`。
> - Default 参照：`frontends/default/src/routes/**`（107 个 route 文件）、`navigation/*`、`api/*`、`components/*`。
> - 后端契约：`apps/api/src/bootstrap/routes.rs`、`apps/api/src/modules/**`、`apps/api/src/core/contract.rs`、`apps/api/src/modules/platform/frontend/**`。
> - 表中每个符号 / 路径都可以用 `rg` 回到真实声明；`## Verification` 记录了核对命令与结果。

## How to use this matrix

> **`Default reference` 列 ≠ 要求。** 该列是**语义**指针（Default 的哪个页面 / API 演示了该行为与边界情况），**不是**必须复刻的路由、页面、导航形态或 UI 组合：Default 的若干页面可以合并成新前端的一个页面，一个 Default 页面也可以拆成多个新页面。`required` 的含义是**该能力对相应用户可用**，不是与 Default 的 UX 对齐。

1. **逐行勾选**：为新前端建一份本表的副本，每实现一项就把该行状态标为 done。未实现的行必须显式声明（部分范围的前端要在交付说明里写清覆盖 / 未覆盖）。
2. **一行算 done 的标准**：该行使用**真实 API 数据**（不是假数据 / 占位），并处理了 **loading / empty / error** 三种状态；分页 / 筛选 / 权限边界按后端语义实现（例如 `EventInfo.joined`、`AwdPlayerStatus.banned`、`AwdpOverview.phase` 必须与后端判定一致）。
3. **完整性 = 能力覆盖，不是路由对齐**：Default 的若干页面可以合并成新前端的一个页面，一个新页面也可以拆成多页；路由路径、布局、导航、组件自由。唯一判据是本文的 `required` 能力是否都有真实实现。
4. **`required` / `optional` / `specialized` 的口径**：
   - `required`：普通选手或运维在默认部署下跑一场赛事（Jeopardy / AWD / AWDP）必然经过的路径；缺一项即为不完整替换。
   - `optional`：完整前端应当提供，但缺失后核心赛程仍可完成（社区附加内容、运维辅助、次要视图）。
   - `specialized`：仅特定运维 / 平台管理场景使用（高权限、基础设施、平台自身配置）。
5. **实时列**：只有真实 SSE / WebSocket 实现才标注；`—` 表示无实时通道，需轮询或按需 refetch。
6. **Public SDK surface 列**：给出可直接 `rg` 的真实符号（`client.service.*` / `client.admin.*` / `client.awd.*` / `client.awdp.*` / `@floatctf/react` hook / `@floatctf/frontend-runtime` 符号）。若某项没有公共 SDK 抽象，写清实际做法；若属于公共面缺口，见 [`## Gaps found`](#gaps-found)。

---

## 平台引导与运行时契约

| Area | Capability | User/Admin | Public SDK surface | Default reference | Realtime | Required for complete frontend |
|---|---|---|---|---|---|---|
| Platform | 公开引导元数据 `GET /api/frontend`（`active_frontend` / `platform_version` / `api_contract_version` / `frontend_runtime_version` / `capabilities`） | platform（bootstrap 提供；前端不实现） | 无 SDK 方法；`@floatctf/frontend-runtime`：`bootstrapFrontend`、`parseBootstrapInfo`、`FloatCTFBootstrapInfo` | `apps/web/src/main.ts` + `frontends/default/src/dev.tsx` | — | required（前端义务 = 正确消费 `mount(context)`）— 取数 / 解析 / 选前端 / 兼容性校验由平台 bootstrap 完成；新前端**不需要**自己 fetch `GET /api/frontend`，只要正确使用 `FloatCTFMountContext` 中已解析的字段；自行读取该元数据属可选增强，不计入完整性 |
| Platform | 前端挂载契约 `mount(context)` 与 `FloatCTFMountContext` 字段（`root` / `apiBaseUrl` / `assetBaseUrl` / `frontendId` / `frontendVersion` / `platformVersion` / `apiContractVersion` / `frontendRuntimeVersion` / `capabilities`） | both | `FloatCTFFrontendModule.mount`、`FloatCTFMountContext`、`normalizeFrontendModule`（frontend-runtime） | `frontends/default/src/entry.tsx` | — | required — 新前端唯一入口契约 |
| Platform | 契约版本常量与兼容性判断 | both | `API_CONTRACT_VERSION`、`FRONTEND_RUNTIME_VERSION`、`FRONTEND_MANIFEST_SCHEMA_VERSION`、`FRONTEND_REGISTRY_SCHEMA_VERSION`、`isMajorCompatible`、`parseMajorConstraint` | `packages/frontend-runtime/src/version.ts` | — | required — 前端与平台契约不匹配时必须能判定并给出诊断 |
| Platform | 制品 manifest / 本地注册表解析 | admin（运维 / 平台管理） | `parseFrontendManifest`、`parseRegistry`、`resolveFrontend`、`listFrontendIds`、`emptyRegistry`（frontend-runtime） | `frontends/default/src/components/admin/FrontendSelector.tsx` | — | optional — 浏览器端只做「选择已安装前端」与兼容性展示，安装由宿主 CLI 完成 |
| Platform | 破窗回退（`?frontend=<id>` / 内置 `default`） | platform（bootstrap 提供） | `BootstrapFrontendOptions.overrideFrontendId`、`fallbackFrontendId`、`renderBootstrapEmergencyUi`（frontend-runtime） | `apps/web/src/main.ts` | — | optional — 由 bootstrap 提供的恢复路径（`overrideFrontendId` / `fallbackFrontendId`）；前端无需实现，也不要求消费 `?frontend=`；不影响正常流程 |
| Platform | 统一响应封装与成功码 | both | `UniResponse<T>`、`QueryParams`、`UNI_SUCCESS_CODE` | `frontends/default/src/components/Table.tsx`（读取 `res.data` / `meta`） | — | required — 所有领域调用都返回该封装，分页依赖 `meta` |
| Platform | 统一错误模型 | both | `FloatCTFError`、`FloatCTFErrorKind`、`FloatCTFErrorResponse`、`toFloatCTFError`、`floatCTFErrorFromEnvelope` | `frontends/default/src/components/apiErrorMessage.ts` | — | required — 错误提示必须来自真实后端消息，不能自造文案 |
| Platform | SSE 传输原语（Bearer 走 `Authorization`，指数退避重连） | both | `client.sse.connect`、`client.sse.connectAdmin`、`client.sse.createParser`、`resolveSseUrl`、`connectSse`、`createSseParser`、`SseConnection` | `packages/react/src/useAwdEventStream.ts` | SSE（通用） | required — AWD / AWDP 实时面板的基础设施 |

> **引导职责归平台（bootstrap-owned）。** `GET /api/frontend` 与本地注册表（`/__floatctf/frontends/registry.json`）的取数、解析、前端选择与兼容性校验都由平台 bootstrap 完成（`apps/web/src/main.ts` → `@floatctf/frontend-runtime` 的 `bootstrapFrontend`）；新前端只需正确实现 `mount(context)` 并消费已解析的 `FloatCTFMountContext` 字段。自行再读该元数据或本地注册表（例如实现前端选择器）属**可选增强**，不计入完整性，不实现不构成缺口。

## 认证与会话

| Area | Capability | User/Admin | Public SDK surface | Default reference | Realtime | Required for complete frontend |
|---|---|---|---|---|---|---|
| Auth | 选手注册 | player | `client.service.users.register` | `frontends/default/src/routes/register.tsx`（`/register`） | — | required — 新选手进入平台的入口 |
| Auth | 选手登录（签发 JWT） | player | `client.service.users.login` | `frontends/default/src/routes/index.tsx`（`/`） | — | required — 选手端必经入口 |
| Auth | 当前用户信息（`GET /users/me`） | player | `client.service.users.getMe` | `frontends/default/src/routes/service/profile.tsx` | — | required — 会话恢复 / 个人资料 / 权限判定 |
| Auth | 修改个人资料 | player | `client.service.users.patchMe` | `frontends/default/src/routes/service/profile.tsx` | — | required — 选手账号基础维护 |
| Auth | 头像上传 | player | `client.service.uploads.upload_avatar` | `frontends/default/src/routes/service/profile.tsx` | — | optional — 展示性资料，缺省不影响赛程 |
| Auth | 忘记密码（发送重置邮件） | player | `client.service.users.resetPassword` | `frontends/default/src/routes/reset_password.tsx`（`/reset_password`） | — | required — 账号自助恢复入口 |
| Auth | 凭 token 重置密码 | player | `client.service.users.reset` | `frontends/default/src/routes/reset.tsx`（`/reset`） | — | required — 用户必须能用重置邮件中的 token 完成改密；页面与路由由前端自定 |
| Auth | 选手登出 | player | 无服务端端点（JWT 无状态）；前端自行清除 token | `frontends/default/src/components/Header.tsx`（`useAuthStore.getState().removeToken()`） | — | required — 选手必须能结束本地会话 |
| Auth | 管理端登录 | admin | `client.admin.login` | `frontends/default/src/routes/admin/index.tsx`（`/admin/`） | — | required — 运维端必经入口 |
| Auth | 管理端登出 | admin | 无服务端端点（JWT 无状态）；前端自行清除 admin token | `frontends/default/src/components/Header.tsx`（`removeAdminToken()`） | — | required — 运维必须能结束本地会话 |
| Auth | 401 统一处理（清对应 token；重新登录的引导方式由前端决定） | both | `FloatCTFClientOptions.onUnauthorized`、`UnauthorizedContext`、`FloatCTFAuthScope` | `frontends/default/src/api/client.ts` | — | required — 令牌过期 / 失效后的统一行为（清对应用户端 token），不得静默失败 |
| Auth | token 注入（选手 / 管理端两个来源） | both | `FloatCTFClientOptions.getUserToken`、`getAdminToken` | `frontends/default/src/api/client.ts` + `frontends/default/src/stores/AuthStore.ts` | — | required — SDK 不持有 token，必须由前端注入 |
| Auth | 非 401 错误回调 | both | `FloatCTFClientOptions.onError`、`requestConfig` | `frontends/default/src/api/client.ts` | — | optional — 统一日志 / 埋点，不影响功能 |

## 选手端：账号与社区内容

| Area | Capability | User/Admin | Public SDK surface | Default reference | Realtime | Required for complete frontend |
|---|---|---|---|---|---|---|
| Community | 全站公告列表 | player | `client.service.announcements.fetch` | `frontends/default/src/routes/service/announcements.tsx` | — | required — 平台级通知是选手常规信息面 |
| Community | 武器库（Arsenal）列表 | player | `client.service.weapons.fetch` | `frontends/default/src/routes/service/weapons.tsx` | — | optional — 附加资料库，不阻塞赛程 |
| Community | 解题流水（分页 / 筛选） | player | `client.service.solves.fetch`（`SolveResult`） | `frontends/default/src/routes/service/solves.tsx` | — | required — 赛事公开动态与题目验证 |
| Community | Top15 用户排行榜 | player | `client.service.solves.getTop15Users`（`TopUser`） | `frontends/default/src/routes/service/top.tsx` | — | required — 用户必须能查看 Top15 排行数据；可并入其他视图，不要求独立页面，也不要求 `/service` 或任何特定路由 |
| Community | 讨论区列表 | player | `client.service.discussions.fetch` | `frontends/default/src/routes/service/discussions/index.tsx` | — | required — 选手社区主入口 |
| Community | 讨论详情 | player | `client.service.discussions.get`（`DiscussionWithAuthor`） | `frontends/default/src/routes/service/discussions/$id.tsx` | — | required — 帖子阅读页 |
| Community | 发帖 / 编辑 / 删除自己的讨论 | player | `client.service.discussions.create`、`patch`、`remove` | `frontends/default/src/routes/service/discussions/my.tsx` | — | required — 选手创建内容的基本闭环 |
| Community | 讨论点赞 / 取消 | player | `client.service.discussions.like`、`unlike` | `frontends/default/src/routes/service/discussions/$id.tsx` | — | optional — 互动增强 |
| Community | 讨论评论 CRUD | player | `client.service.discussions.getComments`、`createComment`、`patchComment`、`deleteComment` | `frontends/default/src/routes/service/discussions/$id.tsx` | — | required — 讨论区核心交互 |

## 选手端：赛事与 Jeopardy

| Area | Capability | User/Admin | Public SDK surface | Default reference | Realtime | Required for complete frontend |
|---|---|---|---|---|---|---|
| Events | 赛事列表（含 `hidden` / 家族 / 赛制过滤） | player | `client.service.events.fetch`（`EventInfo[]`） | `frontends/default/src/routes/service/events/index.tsx` | — | required — 选手进入比赛的第一跳 |
| Events | 赛事详情（`EventInfo.event` / `joined` / `team_result`） | player | `client.service.events.get` | `frontends/default/src/routes/service/events/jeopardy.$id/index.tsx` | — | required — 参赛、题目、积分、实例等赛事能力共同依赖的赛事上下文数据；不要求页面层级或导航结构 |
| Events | 加入 / 退出赛事 | player | `client.service.events.join`、`leave` | `frontends/default/src/routes/service/events/jeopardy.$id/index.tsx` | — | required — 参赛资格与可见性由 `joined` 决定 |
| Events | 战队创建 / 加入 / 退出 | player | `client.service.events.createTeam`、`joinTeam`、`quitTeam` | `frontends/default/src/routes/service/events/jeopardy.$id/index.tsx`、`.../awd.$id/index.tsx` | — | required — 团队赛制（`participant_mode`）参赛必经 |
| Events | 赛事公告 | player | `client.service.events.getAnnouncements`（`EventAnnouncements[]`） | `frontends/default/src/routes/service/events/jeopardy.$id/announcement.tsx` | — | required — 赛程 / 规则变更通知 |
| Events | 赛事积分榜 | player | `client.service.events.getScoreboard`（`ScoreboardItem[]`） | `frontends/default/src/routes/service/events/jeopardy.$id/scoreboard.tsx` | — | required — 比赛核心视图 |
| Events | 赛事趋势（分数随时间） | player | `client.service.events.getTrend`（`TrendItem[]`） | `frontends/default/src/routes/service/events/jeopardy.$id/trend.tsx` | — | required — 比赛核心视图 |
| Events | 赛事实例列表 | player | `client.service.events.getInstances`（`EventInstanceResult[]`） | `frontends/default/src/routes/service/events/jeopardy.$id/instances.tsx` | — | required — 选手管理自己已启动的题目环境 |
| Events | 赛事 writeup 状态 / PDF 提交 | player | `client.service.events.getOwnWp`、`client.service.submit.submitWriteup` | `frontends/default/src/components/SubmitWriteup.tsx` | — | optional — 赛后材料提交，非比赛进行时必经 |
| Jeopardy | 题目目录（挑战列表，分页 / 分类筛选） | player | `client.service.challenges.fetch`（`ChallengesListItem`） | `frontends/default/src/routes/service/challenges/index.tsx` | — | required — Jeopardy 选手主入口 |
| Jeopardy | 题目详情（附件 / 描述 / 分值） | player | `client.service.challenges.get` | `frontends/default/src/routes/service/challenges/$id/index.tsx` | — | required — 解题流程（展示题目 / 获取实例 / 提交 flag）所需的基础数据 |
| Jeopardy | 独立题目实例获取 / 启动 | player | `client.service.challenges.getInstance`、`client.service.instances.launch` | `frontends/default/src/routes/service/challenges/$id/index.tsx` | — | required — 动态题（容器题）解题必经 |
| Jeopardy | 独立题目实例销毁 | player | `client.service.instances.destroy` | `frontends/default/src/routes/service/challenges/$id/index.tsx` | — | required — 释放资源 / 重开环境 |
| Jeopardy | flag 提交（挑战维度） | player | `client.service.submit.submit`（`SolveResult`） | `frontends/default/src/routes/service/challenges/$id/index.tsx` | — | required — 得分核心动作 |
| Jeopardy | 我的实例列表 / 批量销毁 | player | `client.service.instances.fetch`、`bulkDelete` | `frontends/default/src/routes/service/instances.tsx` | — | required — 选手资源管理入口 |
| Jeopardy | 题集（Challenge Sets）列表 / 详情 | player | `client.service.challenges.getChallengeSets`、`getChallengeSet` | `frontends/default/src/routes/service/challenge_sets/index.tsx`、`.../$id/index.tsx` | — | optional — 训练组织方式，非比赛必经 |
| Jeopardy | 题集内题目实例 + flag 提交 | player | `client.service.instances.launch`、`client.service.submit.submit`、`client.service.challenges.getInstance` | `frontends/default/src/routes/service/challenge_sets/$id/index.tsx` | — | optional — 同上 |
| Jeopardy | 我的题解（per challenge）读取 / 保存 | player | `client.service.challenges.getMyWriteup`、`createMyWriteup` | `frontends/default/src/routes/service/challenges/$id/route.tsx` | — | optional — 赛后写作，不阻塞解题 |
| Jeopardy | 题解列表 / 详情（他人 writeups） | player | `client.service.challenges.getWriteups`、`getAllWriteups`、`getWriteup` | `frontends/default/src/routes/service/writeups/index.tsx`、`.../$id.tsx` | — | optional — 学习 / 归档内容 |
| Jeopardy | 赛事题目列表（按赛事开放集合） | player | `client.service.events.fetchChallenges`（`EventChallengeResult[]`） | `frontends/default/src/routes/service/events/jeopardy.$id/challenges.tsx` | — | required — 赛事进行中只应看到已开放题目 |
| Jeopardy | 赛事题目实例获取 / 启动 | player | `client.service.events.getChallengeInstance`、`launchSingleInstance` | `frontends/default/src/routes/service/events/jeopardy.$id/challenges.tsx` | — | required — 赛事内动态题必经 |
| Jeopardy | flag 提交（赛事维度） | player | `client.service.submit.submitSingle` | `frontends/default/src/routes/service/events/jeopardy.$id/challenges.tsx` | — | required — 赛事得分核心动作 |
| Media | Markdown 编辑器图片上传 | player | `client.service.uploads.upload_image` | `frontends/default/src/components/MDPlusEditor.tsx` | — | optional — 讨论 / writeup 插图能力 |

## 选手端：AWD

| Area | Capability | User/Admin | Public SDK surface | Default reference | Realtime | Required for complete frontend |
|---|---|---|---|---|---|---|
| AWD | 赛事状态（`status` / `phase` / `current_round` / `banned` / `score`） | player | `client.awd.player.status`（`AwdPlayerStatus`） | `frontends/default/src/routes/service/events/awd.$id/index.tsx` | — | required — AWD 面板所有状态判定的权威来源 |
| AWD | GameBox 列表与我的实例（IP / 容器 / 健康） | player | `client.awd.player.gameboxes`（`AwdGameBox[]`） | `frontends/default/src/routes/service/events/awd.$id/gameboxes.tsx` | — | required — AWD 攻防目标入口 |
| AWD | GameBox 重置 | player | `client.awd.player.resetGamebox` | `frontends/default/src/routes/service/events/awd.$id/gameboxes.tsx` | — | required — 环境损坏 / 被攻陷后的恢复（受免费次数与罚分约束） |
| AWD | flag 提交（攻击得分） | player | `client.awd.player.submitFlag` | `frontends/default/src/routes/service/events/awd.$id/index.tsx` | — | required — AWD 得分核心动作 |
| AWD | 积分榜（`attack_score` / `defense_score` / `rank`） | player | `client.awd.player.scores`（`AwdScoreRow[]`） | `frontends/default/src/routes/service/events/awd.$id/scoreboard.tsx` | — | required — AWD 核心视图 |
| AWD | WireGuard 配置下发 | player | `client.awd.player.wireguardConfig`（`WireGuardConfigResponse`） | `frontends/default/src/routes/service/events/awd.$id/wireguard.tsx` | — | required — 选手接入 AWD 网络的唯一方式 |
| AWD | 队伍 SSH 凭据（端口 / 密码 / 实例清单） | player | `client.awd.player.sshConfig`（`SshAccessResponse`） | `frontends/default/src/routes/service/events/awd.$id/ssh.tsx` | — | required — 攻击 / 修复 GameBox 的主要通道 |
| AWD | 选手端实时流 | player | `client.sse.connect({ url: "/events/<id>/awd/stream" })`（路径非公共常量，见 Gaps）+ `@floatctf/react` `useAwdEventStream` | `frontends/default/src/routes/service/events/awd.$id/route.tsx` | SSE `GET /events/{id}/awd/stream`（user token） | required — 分数 / 轮次 / 封禁必须实时刷新 |

## 选手端：AWDP 比赛

| Area | Capability | User/Admin | Public SDK surface | Default reference | Realtime | Required for complete frontend |
|---|---|---|---|---|---|---|
| AWDP | 赛事总览（`phase` / 时长配置 / 我的 GameBox / `my_score`） | player | `client.awdp.player.overview`（`AwdpOverview`） | `frontends/default/src/routes/service/events/awdp.$id/-workbench.tsx` | — | required — AWDP 工作台所有状态判定的权威来源 |
| AWDP | 实例启动 / 停止 / 重置 / 查询 | player | `client.awdp.player.startInstance`、`stopInstance`、`resetInstance`、`getInstance` | `frontends/default/src/routes/service/events/awdp.$id/-workbench.tsx` | — | required — Break / Fix 阶段的环境生命周期 |
| AWDP | Break 阶段 flag 提交 | player | `client.awdp.player.submitBreak`（`BreakSubmitResponse`） | `frontends/default/src/routes/service/events/awdp.$id/-workbench.tsx` | — | required — Break 阶段得分核心动作 |
| AWDP | 补丁上传（Fix 阶段） | player | `client.awdp.player.uploadPatch`（`PatchSubmitResponse`） | `frontends/default/src/routes/service/events/awdp.$id/-workbench.tsx` | — | required — Fix 阶段得分核心动作 |
| AWDP | 手工 Test Check | player | `client.awdp.player.testCheck`（`ManualCheckDto`） | `frontends/default/src/routes/service/events/awdp.$id/-workbench.tsx` | — | required — 提交前自检，官方评测前的必经动作 |
| AWDP | 源码下载（presigned URL） | player | `client.awdp.player.sourceUrl` | `frontends/default/src/routes/service/events/awdp.$id/-workbench.tsx` | — | optional — 辅助分析，不影响提交 |
| AWDP | 轮次列表 | player | `client.awdp.player.rounds`（`AwdpRoundDto[]`） | `frontends/default/src/routes/service/events/awdp.$id/rounds.tsx` | — | required — 了解当前 / 历史轮次 |
| AWDP | 我的官方评测结果 | player | `client.awdp.player.evaluations`（`AwdpEvaluationDto[]`） | `frontends/default/src/routes/service/events/awdp.$id/rounds.tsx` | — | required — Fix 阶段得分反馈 |
| AWDP | 我的积分流水（score ledger） | player | `client.awdp.player.scores`（`AwdpScoreRow[]`） | `frontends/default/src/routes/service/events/awdp.$id/scoreboard.tsx`（Default 未单独渲染流水；`-workbench.tsx` 用 `overview.my_score`） | — | optional — SDK 已暴露，Default 仅展示汇总 |
| AWDP | 积分榜（矩阵明细 `AwdpScoreboardDetail`） | player | `client.awdp.player.scoreboard` | `frontends/default/src/routes/service/events/awdp.$id/scoreboard.tsx` | — | required — AWDP 核心视图 |
| AWDP | 趋势 | player | `client.awdp.player.trend`（`AwdpTrendItem[]`） | `frontends/default/src/routes/service/events/awdp.$id/trend.tsx` | — | required — AWDP 核心视图 |
| AWDP | 选手端实时流 | player | `client.sse.connect({ url: "/events/<id>/awdp/stream" })`（路径非公共常量，见 Gaps）+ `@floatctf/react` `useAwdpEventStream` | `frontends/default/src/routes/service/events/awdp.$id/route.tsx` | SSE `GET /events/{id}/awdp/stream`（user token） | required — 阶段切换 / 轮次 / 分数必须实时刷新 |

## 选手端：AWDP Training Ground（练习）

| Area | Capability | User/Admin | Public SDK surface | Default reference | Realtime | Required for complete frontend |
|---|---|---|---|---|---|---|
| AWDP Training | 练习 GameBox 目录 + 开始训练（`capability: "awdp"`） | player | `client.awdp.runs.gameboxCatalog`、`startTraining`（`GameBoxCatalogDto` / `AwdpRunDto`） | `frontends/default/src/routes/service/gameboxes.tsx` | — | required — 选手必须能发现并开始 AWDP 练习；练习是 AWDP 的核心训练方式，不要求特定导航位置或入口形态 |
| AWDP Training | 练习 Run 生命周期（读取 / 开始 / 停止 / 重置 / 结束 / 切阶段 / 重新训练） | player | `client.awdp.runs.getRun`、`startRun`、`stopRun`、`resetRun`、`endRun`、`setPhase`、`restartTraining` | `frontends/default/src/routes/service/awdp/runs.$runId/index.tsx` | — | required — 练习流程主干 |
| AWDP Training | 练习 Run 实例管理 | player | `client.awdp.runs.startInstance`、`stopInstance`、`resetInstance`、`getInstance` | `frontends/default/src/routes/service/awdp/runs.$runId/index.tsx` | — | required — 练习环境生命周期 |
| AWDP Training | 练习 Run 破题 / 补丁 / 自检 / 全量校验 / 源码 | player | `client.awdp.runs.submitBreak`、`uploadPatch`、`testCheck`、`allCheck`、`sourceUrl` | `frontends/default/src/routes/service/awdp/runs.$runId/index.tsx` | — | required — 练习的核心动作集 |
| AWDP Training | 练习 Run 轮次 / 评测 / 积分 | player | `client.awdp.runs.rounds`、`evaluations`、`scores`（`AwdpRunScoresDto`） | `frontends/default/src/routes/service/awdp/runs.$runId/index.tsx` | — | required — 练习反馈回路 |
| AWDP Training | 练习 Run writeup 读写 | player | `client.awdp.runs.getWriteup`、`saveWriteup` | `frontends/default/src/routes/service/awdp/runs.$runId/writeup.tsx` | — | optional — 练习记录，非流程必经 |
| AWDP Training | 练习 Run 实时流 | player | `client.sse.connect({ url: "/service/awdp/runs/<runId>/stream" })` + `@floatctf/react` `useAwdpRunStream` | `frontends/default/src/routes/service/awdp/runs.$runId/index.tsx` | SSE `GET /service/awdp/runs/{runId}/stream`（user token） | required — 练习阶段切换与评测结果需实时刷新 |

## 管理端：总览与内容

| Area | Capability | User/Admin | Public SDK surface | Default reference | Realtime | Required for complete frontend |
|---|---|---|---|---|---|---|
| Admin | Dashboard 聚合总览（统计 / 需关注项 / 赛事 / 动态） | admin | `client.admin.dashboard.summary`（`DashboardSummary`） | `frontends/default/src/routes/admin/dashboard.tsx` | — | required — 运维必须能看到平台聚合状态（统计 / 需关注项 / 赛事 / 动态）；不要求作为默认落地路由或独立页面 |
| Admin | 系统监控（CPU / 内存 / 磁盘 / 网卡 / Docker 概况） | admin | `client.admin.system.monitor`（`SystemInformation`）+ `@floatctf/react` `systemInformationQueryOptions` | `frontends/default/src/routes/admin/dashboard.tsx` | — | optional — 仪表盘增强信息，不影响操作 |
| Admin | 平台版本（API 版本） | admin | `client.admin.system.version` | `frontends/default/src/routes/admin/version.tsx` | — | optional — 诊断信息 |
| Admin | 用户管理 CRUD | admin | `client.admin.users.fetch`、`create`、`patch`、`remove` | `frontends/default/src/routes/admin/users.tsx` | — | required — 平台账号治理基础 |
| Admin | 挑战管理 CRUD + 导入 / 校验 / 构建 / 扫描 | admin | `client.admin.challenges.fetch`、`create`、`patch`、`remove`、`importChallenge`、`checkChallenges`、`buildChallenges`、`scanChallenges` | `frontends/default/src/routes/admin/challenges.tsx` | — | required — 题目是比赛核心资产 |
| Admin | 题集管理 CRUD + 题目增删 | admin | `client.admin.challenges.getChallengeSets`、`createChallengeSet`、`patchChallengeSet`、`deleteChallengeSet`、`getChallengeSet`、`addChallengeToSet`、`removeChallengeFromSet` | `frontends/default/src/routes/admin/challenge_sets/index.tsx`、`.../$id.tsx` | — | optional — 组织方式，非单场赛事必经 |
| Admin | 全局公告 CRUD | admin | `client.admin.announcements.fetch`、`create`、`patch`、`remove` | `frontends/default/src/routes/admin/announcements.tsx` | — | optional — 通知运营，不影响赛程 |
| Admin | 讨论管理（列表 / 删除 / 评论删除） | admin | `client.admin.discussions.fetch`、`get`、`remove`、`getComments`、`removeComment` | `frontends/default/src/routes/admin/discussions.tsx` | — | optional — 社区治理 |
| Admin | 武器库管理 CRUD + 文件上传 | admin | `client.admin.weapons.fetch`、`create`、`patch`、`remove`、`upload` | `frontends/default/src/routes/admin/weapons.tsx` | — | optional — 附加资源管理 |
| Admin | 超管账号 CRUD | admin | `client.admin.super_admin.fetch`、`create`、`patch`、`remove` | `frontends/default/src/routes/admin/super_admins.tsx` | — | optional — 单管理员部署可不使用 |
| Admin | 操作日志查询 | admin | `client.admin.logs.fetch` | `frontends/default/src/routes/admin/logs.tsx` | — | optional — 审计辅助 |
| Admin | 动态设置 CRUD（含受保护键 `FRONTEND_ACTIVE`） | admin | `client.admin.settings.fetch`、`create`、`patch`、`remove`（`SettingsDto`） | `frontends/default/src/routes/admin/settings.tsx` | — | required — 平台运行参数由设置表驱动 |

## 管理端：赛事管理

| Area | Capability | User/Admin | Public SDK surface | Default reference | Realtime | Required for complete frontend |
|---|---|---|---|---|---|---|
| Admin Events | 赛事列表 / 创建 / 编辑 / 删除 | admin | `client.admin.events.fetch`、`create`、`patch`、`remove` | `frontends/default/src/routes/admin/events/index.tsx` | — | required — 赛事创建是运维第一动作 |
| Admin Events | 赛事详情读写（含家族 / 赛制 / `hidden`） | admin | `client.admin.events.get`、`patch` | `frontends/default/src/routes/admin/events/jeopardy.$id/route.tsx` | — | required — 赛事配置中心 |
| Admin Events | 赛事数据大屏（题目 / 解题 / 积分 / 趋势聚合） | admin | `client.admin.events.getData`（`DataPresent`） | `frontends/default/src/routes/admin/events/jeopardy.$id/data_present.tsx` | — | optional — 展示型视图 |
| Admin Events | 赛事题目管理（增删 / 改分 / 开放 / 隐藏） | admin | `client.admin.event_challenges.fetch`、`add`、`setPoints`、`remove`、`open`、`hidden` | `frontends/default/src/routes/admin/events/jeopardy.$id/index.tsx` | — | required — 控制比赛题目集合与可见性 |
| Admin Events | 赛事用户管理（增删 / 封禁 / 解封） | admin | `client.admin.event_users.fetch`、`add`、`delete`、`banned`、`unbanned` | `frontends/default/src/routes/admin/events/jeopardy.$id/users.tsx` | — | required — 参赛名单与作弊处置 |
| Admin Events | 赛事战队管理（列表 / 删除 / 封禁 / 解封） | admin | `client.admin.event_teams.getTeams`、`remove`、`banned`、`unbanned` | `frontends/default/src/routes/admin/events/jeopardy.$id/teams.tsx` | — | required — 团队赛制治理 |
| Admin Events | 赛事公告管理 CRUD | admin | `client.admin.event_announcements.fetch`、`create`、`patch`、`remove` | `frontends/default/src/routes/admin/events/jeopardy.$id/announcements.tsx` | — | required — 赛程通知发布 |
| Admin Events | 赛事日志 | admin | `client.admin.event_logs.fetch` | `frontends/default/src/routes/admin/events/jeopardy.$id/logs.tsx` | — | optional — 审计辅助 |
| Admin Events | 赛事 writeup 列表 + 报告导出 / 下载 | admin | `client.admin.event_writeups.fetch`、`client.admin.events.getReport`、`client.admin.events.exportWriteUps`、`client.admin.download.download` | `frontends/default/src/routes/admin/events/jeopardy.$id/writeups.tsx` | — | optional — 赛后归档 |
| Admin Events | 赛事统一实例列表（challenge + gamebox 归一化） | admin | `client.admin.instances.listForEvent`（`AdminInstanceRow`） | `frontends/default/src/components/admin/EventInstancesTable.tsx` | — | required — 运维排查 / 清理运行中环境 |

## 管理端：AWD 运维

| Area | Capability | User/Admin | Public SDK surface | Default reference | Realtime | Required for complete frontend |
|---|---|---|---|---|---|---|
| Admin AWD | AWD 赛事配置（创建 / 读取 / 更新，乐观锁 `expected_updated_at`） | admin | `client.awd.admin.getStatus`、`createEvent`、`updateConfig`（`AwdEventStatus` / `AwdEventConfigInput`） | `frontends/default/src/routes/admin/events/awd.$id/configure.tsx` | — | required — AWD 赛事参数配置 |
| Admin AWD | 生命周期：deploy / start / pause / resume / finish / archive | admin | `client.awd.admin.deploy`、`start`、`pause`、`resume`、`finish`、`archive` | `frontends/default/src/routes/admin/events/awd.$id/ops.tsx` | — | required — 比赛开闭控制 |
| Admin AWD | 凭据轮换与预检（precheck + 历史 `AwdPrecheckRun`） | admin | `client.awd.admin.rotateTokens`、`precheck`、`prechecks` | `frontends/default/src/routes/admin/events/awd.$id/ops.tsx` | — | required — 开赛前环境校验与密钥轮换 |
| Admin AWD | 分数调整与积分榜 | admin | `client.awd.admin.adjustScore`、`scores`（`AwdScoreRow[]`） | `frontends/default/src/routes/admin/events/awd.$id/ops.tsx` | — | required — 判罚 / 修正与排名核对 |
| Admin AWD | 战队封禁 / 解封 | admin | `client.awd.admin.banTeam`、`unbanTeam` + `client.admin.event_teams.*` | `frontends/default/src/routes/admin/events/awd.$id/teams.tsx` | — | required — AWD 作弊 / 违规处置 |
| Admin AWD | 赛事 GameBox 挂载（列表 / 添加 / 更新 / 移除） | admin | `client.awd.admin.listEventGameboxes`、`addEventGamebox`、`updateEventGamebox`、`removeEventGamebox`（`EventGameBoxDto`） | `frontends/default/src/routes/admin/events/awd.$id/gameboxes.tsx` | — | required — 决定本场 AWD 的题目集合 |
| Admin AWD | GameBox 库管理（导入 / 扫描 / 校验 / 构建 / 隐藏 / 删除 / 更新） | admin | `client.awd.admin.listGameboxes`、`importGamebox`、`updateGamebox`、`hideGamebox`、`removeGamebox`、`scanGameboxes`、`checkGameboxes`、`buildGameboxes` | `frontends/default/src/routes/admin/awd/gameboxes.tsx` | — | required — 题目资产入库与构建 |
| Admin AWD | 赛事网络分配 / 重新分配 | admin | `client.awd.admin.getEventNetwork`、`allocateEventNetwork`、`reallocateEventNetwork`（`EventNetworkInfo`） | `frontends/default/src/routes/admin/events/awd.$id/network.tsx` | — | required — 每个 AWD 赛事开赛前的必做步骤 |
| Admin AWD | 平台网络设置 / 健康 / 分配容量（全局控制面） | admin | `client.awd.admin.getPlatformNetwork`、`updatePlatformNetwork`、`getPlatformNetworkHealth`、`getPlatformNetworkAllocations` | `frontends/default/src/routes/admin/awd/network.tsx` | — | specialized — 平台级基础设施参数，通常一次配置长期不变 |
| Admin AWD | 赛事 GameBox 实例重置 | admin | `client.awd.admin.resetGamebox` | 无 Default 页面（SDK / 后端 `POST /api/admin/events/{id}/awd/gameboxes/{instance_id}/reset`，`apps/api/src/modules/event/awd/api/admin.rs:582`） | — | specialized — 运维应急操作，Default 未提供入口 |
| Admin AWD | 管理端实时流 | admin | `client.sse.connectAdmin({ url: "/events/<id>/awd/stream" })` + `@floatctf/react` `useAdminAwdEventStream` | `frontends/default/src/routes/admin/events/awd.$id/route.tsx` | SSE `GET /events/{id}/awd/stream`（admin token） | optional — 管理台实时刷新，缺失可手动刷新 |

## 管理端：AWDP 运维

| Area | Capability | User/Admin | Public SDK surface | Default reference | Realtime | Required for complete frontend |
|---|---|---|---|---|---|---|
| Admin AWDP | AWDP 赛事配置读写（时长 / 分值 / 乐观锁） | admin | `client.awdp.admin.getConfig`、`updateConfig`（`AwdpEventConfigDto` / `AwdpConfigPatchInput`） | `frontends/default/src/routes/admin/events/awdp.$id/configure.tsx` | — | required — AWDP 赛事参数配置 |
| Admin AWDP | 生命周期：start / break-to-fix / finish | admin | `client.awdp.admin.start`、`breakToFix`、`finish` | `frontends/default/src/routes/admin/events/awdp.$id/ops.tsx` | — | required — 阶段推进控制 |
| Admin AWDP | 赛事 GameBox 挂载 / 卸载 / 列表 | admin | `client.awdp.admin.attachGamebox`、`detachGamebox`、`listEventGameboxes`（`AwdpAdminEventGameBoxDto`） | `frontends/default/src/routes/admin/events/awdp.$id/gameboxes.tsx` | — | required — 决定本场题目集合 |
| Admin AWDP | 赛事实例列表 | admin | `client.awdp.admin.listInstances`（`AwdpAdminInstanceDto`） | `frontends/default/src/routes/admin/events/awdp.$id/instance.tsx`（经 `EventInstancesTable` → `client.admin.instances.listForEvent`） | — | required — 运维排查运行中容器 |
| Admin AWDP | 积分榜 | admin | `client.awdp.admin.scores`（`AwdpScoreRow[]`） | `frontends/default/src/routes/admin/events/awdp.$id/scoreboard.tsx` | — | required — 排名核对 |
| Admin AWDP | 数据大屏 | admin | `client.awdp.admin.dataPresent`（`AwdpDataPresent`） | `frontends/default/src/routes/admin/events/awdp.$id/data_present.tsx` | — | optional — 展示型视图；赛事级实时数据还可用管理端 AWD 流 |

## 管理端：基础设施、系统与平台设置

| Area | Capability | User/Admin | Public SDK surface | Default reference | Realtime | Required for complete frontend |
|---|---|---|---|---|---|---|
| Admin Infra | Docker 容器管理（列表 / 启动 / 停止 / 删除） | admin | `client.admin.docker.fetchContainers`、`startContainer`、`stopContainer`、`deleteContainer`（`FloatDockerContainer` / `PortInfo`） | `frontends/default/src/routes/admin/docker/index.tsx` | — | specialized — 低层容器运维，普通赛事流程不经过 |
| Admin Infra | Docker 镜像管理（列表 / 删除） | admin | `client.admin.docker.fetchImages`、`deleteImage`（`ImageInfo`） | `frontends/default/src/routes/admin/docker/images.tsx` | — | specialized — 镜像清理 |
| Admin Infra | Docker 网络管理（列表 / 创建 / 删除） | admin | `client.admin.docker.fetchNetworks`、`createNetwork`、`deleteNetwork`（`NetworkInfo`） | `frontends/default/src/routes/admin/docker/networks.tsx` | — | specialized — 底层网络运维 |
| Admin Infra | SQL 控制台（多语句执行） | admin | `client.admin.database.exec_sql`（`SqlStatement` / `SqlResult`） | `frontends/default/src/routes/admin/database.tsx` | — | specialized — 高权限逃生工具 |
| Admin Infra | 计划任务 CRUD + 手动运行 | admin | `client.admin.scheduled_tasks.fetch`、`create`、`patch`、`remove`、`run` | `frontends/default/src/routes/admin/scheduled_tasks.tsx` | — | optional — 定时清理 / 维护任务管理 |
| Admin Infra | Web 终端（session 授权 + WebSocket 交互） | admin | **无 SDK 抽象**：`client.adminHttp.post("/terminal/session")` + 原生 `WebSocket`（见 Gaps） | `frontends/default/src/routes/admin/terminal.tsx` | WebSocket `GET {adminBase}/terminal/ws` | specialized — 高权限宿主 shell，仅运维应急使用 |
| Admin Platform | 已安装前端选择器（读取本地注册表 + 写 `FRONTEND_ACTIVE`） | admin | `client.admin.settings.fetch`、`patch` + `fetch(DEFAULT_REGISTRY_URL)`、`parseRegistry`、`FRONTEND_RUNTIME_VERSION`、`API_CONTRACT_VERSION`（frontend-runtime） | `frontends/default/src/components/admin/FrontendSelector.tsx` | — | specialized — 平台自身配置：切换生效前端 |
| Admin Platform | 静态制品下载（presigned URL → Blob） | admin | `client.admin.download.download` | `frontends/default/src/routes/admin/events/jeopardy.$id/writeups.tsx` | — | optional — 报告 / writeup 附件下载 |

---

## Gaps found

### 缺口分类政策（A / B / C）

`PUBLIC SDK GAP` **不等于**「新 Frontend 必须模仿 Default 的 UX」；它的含义是**公共集成面不完整**。逐项按下列类别定性：

| 类别 | 含义 | 前端作者的处置 |
|---|---|---|
| **A** | 用公共包即可干净实现（`@floatctf/sdk` / `@floatctf/frontend-runtime`，React 可选 `@floatctf/react`） | 直接用公共 API 实现；不构成缺口 |
| **B** | 当前需要**有文档的低层逃生舱**：`client.adminHttp` / `client.serviceHttp` / `client.transport.*`、原生 `WebSocket`、同源 `fetch` 本地注册表 | **允许**：可用逃生舱实现，但**必须在交付说明里写明用了哪个逃生舱**；该能力仍算完整 |
| **C** | 从受支持的公共契约**无法安全实现** | Agent **必须**报告 `PUBLIC SDK GAP: <细节>` 并**停止该项能力**：不得自动改后端，不得臆造 SDK 方法 |

- **不得臆造 SDK 方法**：公共包里没有的符号就是没有，不要假设 `client.<domain>.<method>` 存在。
- 逃生舱的可用写法见 [`AI-FRONTEND-GUIDE.md`](./AI-FRONTEND-GUIDE.md) **§5.4 逃生舱（低层公共接口，合法但必须记录）**。

> 口径：Default Frontend 已实现、但公共 SDK 没有抽象（被迫走 `client.adminHttp` / `client.transport.*` / 原生 API / 裸 `fetch`），或 SDK 明显缺少公共符号。以下每条缺口都标注类别（A / B / C）；**以下只报告，不修代码。**

**PUBLIC SDK GAP（class B）：Web 终端没有 SDK 抽象。** 后端 `POST /api/admin/terminal/session`（`apps/api/src/modules/platform/operations/terminal.rs:168`）签发一次性 HttpOnly ticket cookie，随后 `GET /api/admin/terminal/ws`（同文件 `:213`）升级为 WebSocket。SDK 既没有 `client.admin.terminal.*`，也没有 WS 抽象；Default 只能 `await client.adminHttp.post("/terminal/session")`（`frontends/default/src/routes/admin/terminal.tsx:52`）并自行 `new WebSocket(...)`（同文件 `:55`），还要自己实现 `{type:"resize"}` 消息与二进制帧处理（`:58`–`:107`）。新前端若需要终端能力，必须复用同一逃生舱并复刻 WS 协议。**类别 B**：`client.adminHttp` + 原生 `WebSocket` 是受支持的逃生舱，实现后必须在交付说明中声明；不计为 C。

**PUBLIC SDK GAP（class B）：本地前端注册表缺少「取数 + 解析」的公共封装。** 公共包只提供 URL 常量 `DEFAULT_REGISTRY_URL` 与解析器 `parseRegistry`（`packages/frontend-runtime/src/version.ts`、`registry.ts`）；取数必须由前端自己 `fetch`（Default：`frontends/default/src/components/admin/FrontendSelector.tsx:56`）。注册表是同源静态文件、不是后端 API，因此 SDK 未覆盖可以理解，但对「第三方前端实现前端选择器」而言没有一行式公共助手。**类别 B**：同源 `fetch` + `parseRegistry` 是受支持的逃生舱；且该能力本身在表内为 `optional`，不实现不影响完整性。

**PUBLIC SDK GAP（class B）：SSE 流路径不是公共符号。** `@floatctf/sdk` 只导出通用 `client.sse.connect` / `connectAdmin` / `createParser` 与 `resolveSseUrl`；四条真实流的 URL 只硬编码在 `@floatctf/react` 的 hook 内：`/events/{id}/awd/stream`（`packages/react/src/useAwdEventStream.ts:161`、`useAdminAwdEventStream.ts:136`）、`/events/{id}/awdp/stream`（`packages/react/src/useAwdpEventStream.ts:131`）、`/service/awdp/runs/{runId}/stream`（`packages/react/src/useAwdpRunStream.ts:112`）。**非 React 前端不受益于 `@floatctf/react`**，必须自己拼这些路径字符串，且没有编译期保护。**类别 B**：路径字符串 + `client.sse.connect({ url })` 是受支持的逃生舱，非 React 前端同样可用，需在交付说明中声明。

**PUBLIC SDK GAP（逐项类别 A / B）：后端存在、SDK 未暴露的端点（Default 均未使用，因此不影响 Default 的能力覆盖，但限制第三方前端在不用逃生舱时的自由度）：**
- **[B]** `GET /api/events/{event_id}/capabilities`（`apps/api/src/modules/event/common/api/player.rs:62`，返回 `EventCapabilities`）——按赛事 `mode` 探测能力，SDK 无对应方法；逃生舱：`client.serviceHttp.get("/events/<id>/capabilities")`（`packages/sdk/src/client.ts:80`）。公共 TS 类型 `Events` 不含 `mode`（`packages/sdk/src/entity/events.ts`），能力映射在后端 `EventCapabilities::for_mode`（`apps/api/src/modules/event/common/domain/capability.rs`），故不能由公共数据自算。
- **[A]** `POST /api/events/{event_id}/team/{team_id}/leave`（同文件 `:206`）——SDK 已封装等效的 `client.service.events.quitTeam`（`DELETE /events/{id}/team/{team_id}`，`packages/sdk/src/api/service/events.ts:55`），该能力可用公共方法完整实现。
- **[B]** `GET /api/instances/{instance_id}`（`apps/api/src/modules/event/jeopardy/api/instances.rs:332`）——SDK 只封装列表 / 启动 / 删除；逃生舱：`client.serviceHttp.get("/instances/<id>")`，公共面另有 `client.service.instances.fetch` 与 `client.service.challenges.getInstance` 可部分替代。
- **[B]** `GET /api/admin/events/{event_id}/awd/judge`（`apps/api/src/modules/event/awd/api/admin.rs:657`）——AWD 判题状态，SDK `client.awd.admin` 无对应方法；逃生舱：`client.adminHttp.get("/admin/events/<id>/awd/judge")`。
- **[B]** `GET /api/admin/events/{event_id}/awdp/runs`（`apps/api/src/modules/event/awdp/api/admin.rs:213`）——AWDP 赛事 run 列表，SDK `client.awdp.admin` 无对应方法；逃生舱：`client.adminHttp.get("/admin/events/<id>/awdp/runs")`。
- **[B]** `POST /api/admin/events/{event_id}/teams`、`GET|POST|DELETE /api/admin/events/{event_id}/teams/{team_id}[/users]`（`apps/api/src/modules/event/common/api/event_teams.rs:24`、`:247`、`:301`、`:388`）——建队与队内成员增删，SDK `client.admin.event_teams` 只有 `getTeams` / `remove` / `banned` / `unbanned`；逃生舱：`client.adminHttp` 的 `get` / `post` / `delete`。
- **[B]** 单条读取端点：`GET /api/admin/logs/{log_id}`（`platform/operations/logs.rs:74`）、`GET /api/admin/scheduled_tasks/{task_id}`（`platform/operations/scheduled_tasks.rs:346`）、`GET /api/admin/users/{user_id}`（`identity/user/mod.rs:292`）、`GET /api/admin/super_admin/{super_user_id}`（`identity/administrator/mod.rs:235`）、`GET /api/admin/events/{event_id}/announcements/{announcement_id}`（`event/common/api/event_announcements.rs:152`）——逃生舱：`client.adminHttp.get(...)`。

**结论：本清单没有 C。** 上述每一项都能由公共方法（A）或受支持的逃生舱（B）触达；当前审计未发现「从受支持的公共契约无法安全实现」的能力。

除以上各项外，**不存在**其他「Default 已实现但公共 SDK 未覆盖」的能力。逃生舱扫描命令与真实输出：

```bash
grep -rnE 'transport\.|serviceHttp|adminHttp|new WebSocket|new EventSource' \
  frontends/default/src --include=*.ts --include=*.tsx | grep -v __tests__
# frontends/default/src/routes/admin/terminal.tsx:52: await client.adminHttp.post("/terminal/session");
# frontends/default/src/routes/admin/terminal.tsx:55: const ws = new WebSocket(buildWsUrl());
grep -rnE '\bfetch\(' frontends/default/src --include=*.ts --include=*.tsx | grep -v __tests__
# 唯一的裸 fetch：frontends/default/src/components/admin/FrontendSelector.tsx:56
```

## Counts

<!-- 本区块由 /tmp/matrix-verify.py 对表体逐行统计后写入（统计时跳过表头与分隔行）。 -->

- **总行数：127**
- `required`：**89**
- `optional`：**30**
- `specialized`：**8**
- **带实时（SSE / WebSocket）的行：6**

分域行数：

| 域（本文 `##` 章节） | 行数 |
|---|---|
| 平台引导与运行时契约 | 8 |
| 认证与会话 | 13 |
| 选手端：账号与社区内容 | 9 |
| 选手端：赛事与 Jeopardy | 23 |
| 选手端：AWD | 8 |
| 选手端：AWDP 比赛 | 12 |
| 选手端：AWDP Training Ground（练习） | 7 |
| 管理端：总览与内容 | 12 |
| 管理端：赛事管理 | 10 |
| 管理端：AWD 运维 | 11 |
| 管理端：AWDP 运维 | 6 |
| 管理端：基础设施、系统与平台设置 | 8 |
| **合计** | **127** |

6 条实时行：

1. 平台引导与运行时契约 — SSE 传输原语（`client.sse.*` / `connectSse`）
2. 选手端：AWD — 选手端实时流（`SSE GET /events/{id}/awd/stream`，user token）
3. 选手端：AWDP 比赛 — 选手端实时流（`SSE GET /events/{id}/awdp/stream`，user token）
4. 选手端：AWDP Training Ground — 练习 Run 实时流（`SSE GET /service/awdp/runs/{runId}/stream`，user token）
5. 管理端：AWD 运维 — 管理端实时流（`SSE GET /events/{id}/awd/stream`，admin token）
6. 管理端：基础设施、系统与平台设置 — Web 终端（`WebSocket GET {adminBase}/terminal/ws`）

## Verification

核对脚本（均从仓库根 `/home/fb0sh/Projects/floatctf` 运行；脚本置 `/tmp`，不写入仓库）：

```bash
python3 /tmp/matrix-verify.py    # 行数/分类/实时 + Default 路径存在性 + SDK 符号 + 实时实现
python3 /tmp/matrix-verify2.py   # Public SDK surface 列每个反引号符号逐一解析 + required 行理由
```

结果（`/tmp/matrix-verify.py`）：

```
rows=127 required=89 optional=30 specialized=8
realtime_rows=6
default_files_checked=129 missing=0
default_routes_checked=4 missing=0
sdk_symbols_checked=119 unresolved=0
unclassified_rows=0 realtime_mismatch=0
exit=0
```

结果（`/tmp/matrix-verify2.py`）：

```
public_surface_tokens=292 unresolved=0
required_rows_without_reason=0
```

逐项核对：

1. **Default 参照路径全部存在。** 脚本对 `Default reference` 列内每个 `` `frontends/…` ``、`` `apps/…` ``、`` `packages/…` `` 反引号路径做 `Path.is_file()`：**129 个路径，0 缺失**。另对行内以反引号给出的 Default 路由路径（`/register`、`/reset_password`、`/reset`、`/admin/`）与 107 个 route 文件的 `createFileRoute("…")` 声明集合比对：**4 个全部命中，0 缺失**（其余路由在文档中以文件名形式给出，已按文件核对）。
2. **Public SDK 符号全部可解析。** `matrix-verify2.py` 提取 `Public SDK surface` 列的 292 个唯一反引号 token，逐一在 `packages/sdk/src/**`、`packages/react/src/**`、`packages/frontend-runtime/src/**` 的声明语料里查找（点路径按后缀逐级回退）：**0 未解析**。保留的允许清单只含包名（`@floatctf/react`、`@floatctf/frontend-runtime`）、浏览器 API（`WebSocket`）与逃生舱写法（`client.adminHttp.post(…)`、`fetch(DEFAULT_REGISTRY_URL)`）。
3. **实时标记都有真实实现。** AWD 选手 / 管理端流对应 `packages/react/src/useAwdEventStream.ts:161` 与 `useAdminAwdEventStream.ts:136`（`/events/${eventId}/awd/stream`）；AWDP 赛事流对应 `useAwdpEventStream.ts:131`；练习 Run 流对应 `useAwdpRunStream.ts:112`；底层 `fetch`+`ReadableStream` + `text/event-stream` 见 `packages/sdk/src/sse/connectSse.ts:121`、`:197`。Web 终端行对应 `frontends/default/src/routes/admin/terminal.tsx:55` 的 `new WebSocket(...)`。默认使用它们的页面：`routes/service/events/awd.$id/route.tsx`、`routes/service/events/awdp.$id/route.tsx`、`routes/service/awdp/runs.$runId/index.tsx`、`routes/admin/events/awd.$id/route.tsx`。**0 处不匹配**。
4. **`required` 行都带理由。** 脚本断言每个 `required` 行的第 7 列在 `required` 之后含 `—` 与理由文本：**0 例外**；理由文字都指向普通选手 / 运维工作流（参赛、解题、提交、开赛、判罚、环境生命周期等）。
5. **逃生舱扫描（Gaps 的依据）。**

```bash
grep -rnE 'transport\.|serviceHttp|adminHttp|new WebSocket|new EventSource' \
  frontends/default/src --include=*.ts --include=*.tsx | grep -v __tests__
# → 仅 2 行，均在 routes/admin/terminal.tsx:52 / :55
grep -rnE '\bfetch\(' frontends/default/src --include=*.ts --include=*.tsx | grep -v __tests__
# → 唯一的裸 fetch 是 components/admin/FrontendSelector.tsx:56（本地注册表静态文件）
```

本轮审计中**被修正**的行 / 结论：

| 初始判断（错误） | 修正后 | 依据 |
|---|---|---|
| Top 榜调用 `serviceApi.solves.getTop` | `client.service.solves.getTop15Users` | `packages/sdk/src/api/service/solves.ts` 的方法名 |
| AWDP 有「赛事级 writeup」能力 | 删除赛事级 writeup 行；writeup 只有 **run 级** `client.awdp.runs.getWriteup` / `saveWriteup` | `routes/service/awdp/runs.$runId/writeup.tsx` + `packages/sdk/src/api/awdpRuns.ts` |
| AWDP 管理端有官方评测（evaluations）接口 | 不建行：SDK `client.awdp.admin` 无 evaluations 方法，Default 也无对应页面 | `packages/sdk/src/api/awdp.ts`（admin 工厂仅 11 个方法） |
| 「我的积分流水」应为 `required` | 降为 `optional` | Default 从未直接调用 `awdpPlayerApi.scores`（`grep` 仅命中 `-workbench.tsx` 用 `overview.my_score`） |
| 赛事 writeup 提交应为 `required` | 降为 `optional` | 属赛后材料，不阻塞比赛流程（`components/SubmitWriteup.tsx`） |
| 管理端 AWDP 实例走 `client.awdp.admin.listInstances` | Default 实际走 `client.admin.instances.listForEvent`（归一化实例表） | `components/admin/EventInstancesTable.tsx:93` |
| 赛事实例列表走 `client.service.instances.fetch` | 选手端赛事实例走 `client.service.events.getInstances` | `routes/service/events/jeopardy.$id/instances.tsx` |
| `GET /events/{id}/capabilities` 应在能力表建行 | 移入 Gaps：后端存在、SDK 未暴露、Default 未使用 | `apps/api/src/modules/event/common/api/player.rs:62`；Default 仅在 `dev.tsx` 硬编码平台级 capabilities |

未决问题及解决方式：`client.awdp.admin.listInstances`、`client.awdp.player.getInstance`、`client.awdp.player.scores`、`client.awd.admin.resetGamebox` 在 Default 中**没有调用点**（`grep -rn 'awdpAdminApi\.\|awdpPlayerApi\.\|awdAdminApi\.' frontends/default/src`）。处置：保留为 SDK 已暴露的能力行并显式标注 Default 未使用（AWDP 实例、积分流水、AWD 实例重置），不臆造页面也不改代码。

### scope-clarification pass（口径收窄，行数 / 分类不变）

本 pass 只做「能力语义 ≠ Default UX」的口径收窄与 bootstrap 归属澄清，**没有增删表格行，也没有改动任何 `required` / `optional` / `specialized` 分类**。

| 项 | 改前 | 改后 |
|---|---|---|
| 总行数 | 127 | **127** |
| `required` / `optional` / `specialized` | 89 / 30 / 8 | **89 / 30 / 8** |
| 实时（SSE / WebSocket）行 | 6 | **6** |
| `Default reference` 路径存在性 | 129 检查 / 0 缺失 | **129 检查 / 0 缺失** |
| `required` 行带理由 | 89 / 89 | **89 / 89** |

改动的行（其余行保持字节不变；`Default reference` 列一律未动）：

1. **bootstrap 归属**：`GET /api/frontend` 行 `User/Admin` 由 `public（未认证）` 改为 `platform（bootstrap 提供；前端不实现）`，第 7 列改为 `required（前端义务 = 正确消费 \`mount(context)\`）— …新前端**不需要**自己 fetch \`GET /api/frontend\`…`；「破窗回退」行 `User/Admin` 由 `both` 改为 `platform（bootstrap 提供）`。平台表后新增「引导职责归平台」说明。`mount(context)` 行**仍为 `required`**（L35，`required — 新前端唯一入口契约`）。
2. **去掉 `required` 里的 UX 规定**（7 行）：凭 token 重置密码（原「重置邮件落地页」）、401 统一处理（原 Capability「+ 回该端登录入口」）、Top15（原「Default 选手端落地页（`/service` 重定向至此）」）、赛事详情（原「所有赛事子页面的上下文」）、题目详情（原「解题页面基础数据」）、AWDP 练习入口（原「选手端导航中的一等入口」）、Admin 总览（原「运维落地页」）。
3. **`## Gaps found`** 增加 A / B / C 分类政策；已有缺口逐条标注类别。结论：**本清单没有 C** —— Web 终端、本地注册表 `fetch`、SSE 流路径为 **B**；端点清单中 `POST …/team/{team_id}/leave` 为 **A**（`client.service.events.quitTeam` 已覆盖），其余为 **B**（`client.serviceHttp` / `client.adminHttp` 可达）。
4. **`## How to use this matrix`** 增加「`Default reference` 列 ≠ 要求」说明。

改后重跑核对命令：

```bash
python3 /tmp/matrix-verify.py    # rows=127 required=89 optional=30 specialized=8
                                 # realtime_rows=6 / default_files_checked=129 missing=0
                                 # default_routes_checked=4 missing=0 / sdk_symbols_checked=119 unresolved=0
                                 # unclassified_rows=0 realtime_mismatch=0  exit=0
python3 /tmp/matrix-verify2.py   # public_surface_tokens=292 unresolved=0 / required_rows_without_reason=0
```

本 pass 的 UX 规定措辞扫描（只看 Capability 与第 7 列，`Hit` 全部是显式否定句「不要求…」）：

```bash
python3 - <<'EOF'   # 正则：必须提供…页面 / 侧边栏 / 入口放在 / 同 Default 一样 / 分页签 / 多页结构 / 落地页 / 子页面 / 解题页面 / 默认落地路由
# 结果：0 条真实规定；3 条命中均为 `不要求独立页面` / `不要求作为默认落地路由` 这类否定表述
EOF
```
