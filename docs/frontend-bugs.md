# 前端问题台账（shadcn 控制台 + 真实 API + 开发 mock + 工具链）

> **适用范围**：可插拔前端 `shadcn`（`frontends/floatctf-frontend-shadcn`，monorepo submodule）、
> 仅供开发预览的 `scripts/mock-api.mjs`，以及本轮目标执行期间实际踩到的
> **真实 dev API 验收 / 开发过程 / 工具链**问题。
> 不涉及 `frontends/default`。
>
> **这份文档解决什么问题**：把 50 轮走查（2026-11-07/08，前后两阶段：开发 mock + 真实 dev API）
> 中发现的“不合理 / 逻辑不对 / 细节缺失”固化成可检索的条目——每条都写清
> **现象、根因、当时怎么修、以后怎么提前规避与检测**。
> mock 阶段的产品/代码缺陷见 §1–§8；真实 API 阶段新增的问题、文件校验、敏感值、
> 生命周期闸门与后端契约边界见 §8A；开发过程与工具链坑见 §11
> （含两次数据丢失事故复盘）。新功能开发、代码评审、联调、写测试时，先按 §0 和 §9
> 的清单过一遍，再动手。
>
> **相关文档**
> - 逐轮台账：[`frontends/floatctf-frontend-shadcn/docs/qa/AUDIT-50.md`](../frontends/floatctf-frontend-shadcn/docs/qa/AUDIT-50.md)
> - 真实 API 验收：[`frontends/floatctf-frontend-shadcn/docs/E2E-ACCEPTANCE.md`](../frontends/floatctf-frontend-shadcn/docs/E2E-ACCEPTANCE.md)
> - 作业口径与 SDK 陷阱：[`frontends/floatctf-frontend-shadcn/docs/dev/CONVENTIONS.md`](../frontends/floatctf-frontend-shadcn/docs/dev/CONVENTIONS.md)
> - 视觉守则：[`docs/theme.md`](theme.md)

---

## 0. 先记住这 18 条（最高频雷区）

| # | 雷区 | 一句话规避 |
|---|------|-----------|
| 1 | `call()` 用在返回 `data:null` 的接口上 | 写接口用 `callVoid`，合法空值用 `callMaybe`，不要用 `call<null>` |
| 2 | 后端/ mock 没实现的写接口“假成功” | mock 的写接口必须 501；前端 toast 成功前要确认列表真的变了 |
| 3 | `filter` / `page` / `limit` 被忽略 | 列表页每加一个筛选/分页，先确认对应 mock 路由真的切片/过滤 |
| 4 | 统计数字与列表不同源 | 概览/大屏的计数从同一份数据算，禁止硬编码 |
| 5 | 枚举直接渲染 | `running` / `Challenge` / `INFO` 一律本地化，未知值原样兜底 |
| 6 | 破坏性操作只靠后端拦 | `useConfirm` + 真实后果 + `confirmPhrase`；受保护项前端也要禁用 |
| 7 | 排序表头看起来像全局排序 | 后端无排序参数时，表头标注“仅当前页排序” |
| 8 | 空态只有“没有数据” | 区分「还没有内容」与「筛选没有匹配」，后者给清除动作 |
| 9 | 焦点/可访问性靠肉眼 | 跳过链接、弹层关闭后焦点归位、`aria-sort`/`aria-label`、隐藏 input 可键盘触达 |
| 10 | mock 状态重启即重置 | 演示前重启 mock 会丢数据；写操作要“能刷新后仍在”才算验证通过 |
| 11 | 移动端表格溢出 | 390px 必测；列用 `hideBelow`，卡片用移动端专用结构 |
| 12 | i18n 动态 key 漏翻译 | 动态拼接的 key（namespace + 枚举值）每个取值都要在 zh/en 同时存在 |
| 13 | 把 SDK 返回类型当成运行时保证 | 真实 API 可能 `data:null`；写操作统一 `callVoid`，可空读用 `callMaybe`，用返回体前先看后端 handler |
| 14 | 只校验 MIME / 后缀，不校验内容 | 图片做真实解码；PDF 精确匹配前 5 字节 `%PDF-`；ZIP 查 `PK`；补丁查 gzip `1f 8b` |
| 15 | 敏感值只在列表遮蔽 | 编辑输入也要默认 password + 显隐按钮；解析值、关联展示同样用 `SecretValue` |
| 16 | 直接按 `is_configurable` 开放按钮 | 以后端真实状态机为准；失败后刷新状态，给恢复路径，不用前端猜 |
| 17 | 把赛事 `upcoming` 当成赛制未运行 | 选手端/赛事页先探测 AWD status、AWDP `started_at`；提前开赛要锁报名并显示徽章 |
| 18 | 只把 404 当资源不存在 | 非法 UUID 后端返回 400「路径参数格式错误」；每个 shell 都要有 not-found 分支 |

---

## 1. 契约与数据层（A）

> 这一节的坑几乎都表现为“页面某块打不开 / 永远加载 / 报「没有返回数据」”。

### A1. `call` / `callVoid` / `callMaybe` 用错（最高频）

| 项 | 内容 |
|---|---|
| **现象** | 点“添加成员 / 加入题集 / 隐藏 GameBox / 提交 flag / 封禁成员”等，toast 报「XXX 没有返回数据」，操作必然失败。 |
| **根因** | 接口返回类型是 `UniResponse<null>`（成功即无数据）。`unwrap` 把 `data:null` 归一成 `undefined`，`call()` 见到 `undefined` 就按“契约违约”抛错。`call<null>(...)` 只是类型标注，运行时仍然抛。 |
| **解决** | 写操作统一 `callVoid(...)`；真实“空是合法态”的查询用 `callMaybe(...)`；只有确实返回对象的接口才用 `call(...)`。已修：添加成员、移出成员、封禁/解封、题目加入题集、隐藏 GameBox、提交 flag、赛事战队 ban/unban、训练实例启停等。 |
| **规避·检测** | ① 新增 mutation 时先看 SDK 返回类型；② `rg 'call<null>' src` 必须为空；③ `rg 'call\(\s*client' src -n` 逐个确认是否可能返回 null；④ mock 的写接口若返回 `data:null`，页面必须用 `callVoid` 才不报错。 |

### A2. `Query data cannot be undefined`（React Query v5）导致白屏

| 项 | 内容 |
|---|---|
| **现象** | 路由白屏，控制台 `Query data cannot be undefined`，页面无错误边界提示。 |
| **根因** | `queryFn` 返回 `undefined`（接口字段缺失/`own_wp` 非字符串/管理端数据页缺数组等）；或对可能为空的接口用了 `call` 之外的裸取。 |
| **解决** | 严格 `call` + `callMaybe`；列表默认 `?? []`；可选字段做类型收窄；`QueryState` 兜底错误。已修：`UserAvatar` 的 `name` 缺省、管理端赛事数据页数组缺省、`own_wp` 类型、AWD/AWDP 数据页 `gameboxes/recent_activity/scoreboard_top10/trend` 防御。 |
| **规避·检测** | ① 每个 `queryFn` 都要能返回确定类型；② `rg '\?\?\(\[\]\|\?\? \[\]'` 看列表是否兜底；③ 用“缺字段”的 mock 数据跑一次页面。 |

### A3. `EventInfo.id` 运行时不返回

| 项 | 内容 |
|---|---|
| **现象** | 赛事页拿到 `undefined` 当 id，请求 404 / 列表不刷新。 |
| **根因** | 运行时 DTO 只有 `event`，没有顶层 `id`（类型里保留是历史兼容）。 |
| **解决** | 一律 `eventIdOf(info)`（取 `info.event.id`）。 |
| **规避·检测** | `rg 'info\.id\b' src` 应无命中；评审时看到 `EventInfo` 就先看 id 来源。 |

### A4. 柯里化 SDK 方法只调一次

| 项 | 内容 |
|---|---|
| **现象** | `fetch(eventId)` 被当成 Promise，页面永远 loading 或类型报错。 |
| **根因** | 部分 SDK 是 `fetch(id) => (params) => Promise`（如 `event_users.fetch`、`event_logs.fetch`、`event_writeups.fetch`、`challengeSet`）。 |
| **解决** | 二次调用：`client.admin.event_logs.fetch(eventId)({ page, limit })`。 |
| **规避·检测** | 看 SDK 签名；`rg '\.fetch\([^)]*\)\.then' src` 之类可疑调用；类型检查会兜住大部分。 |

### A5. 误以为后端支持排序参数

| 项 | 内容 |
|---|---|
| **现象** | 用户点表头以为“全表排序”，实际只排了当前页；或前端发了 `sort` 参数被后端忽略。 |
| **根因** | `QueryParams` 只有 `offset/limit/page/total/filter`，后端没有排序参数；DataTable 排序是**当前页视图内**排序。 |
| **解决** | 可排序表头统一 `title="排序仅作用于当前页；跨页整体排序需要后端支持。"`；列表页脚注说明后端默认排序（如 `updated_at desc`）。 |
| **规避·检测** | 新增表格若想“全局排序”，先确认后端契约；否则保留提示。`rg 'sortValue' src` 时同步检查 `title`。 |

### A6. 百分比：后端给比例，前端再乘 100

| 项 | 内容 |
|---|---|
| **现象** | 解出率显示 `10000.0%` / `7380.0%`。 |
| **根因** | 后端 `solved_percent` 是 **0..1 比例**；`formatPercent()` 内部 ×100。mock 曾直接返回百分数，被再乘一次。 |
| **解决** | 接口字段按比例返回；展示统一走 `formatPercent`；mock 与后端保持同一口径。 |
| **规避·检测** | `rg 'solved_percent\|formatPercent' src`；任何“百分比”字段先问：是 0..1 还是 0..100。 |

### A7. 分数/合计的四舍五入不一致

| 项 | 内容 |
|---|---|
| **现象** | AWD 积分榜 `攻击 2426 + 防守 1985 = 4411`，合计却显示 4410。 |
| **根因** | 两个分量各自 `Math.round(score * 0.55/0.45)`，两个 .5 同时进位，和比总分多 1。 |
| **解决** | 合计为权威值；防守分用 `total - attack` 反推，保证 `attack + defense === total`。 |
| **规避·检测** | 任何“分项 + 分项 = 合计”的展示，用脚本校验所有行；mock 与真实后端都适用。 |

### A8. 枚举大小写、别名与未知值

| 项 | 内容 |
|---|---|
| **现象** | 预检状态 `SUCCESS` 与 `success` 显示不一致；AWD 阶段/实例状态出现原始英文；未知值渲染成空白。 |
| **根因** | 后端枚举大小写不统一；前端直接渲染 `row.status`。 |
| **解决** | 先 `toLowerCase()` 归一化，再映射到 i18n key；未知值原样显示（不要崩溃、不要空白）。已修：日志级别、实例类型/状态、AWD 阶段/状态、预检触发方式、训练目录状态（pending/ended 不再显示“进行中”）。 |
| **规避·检测** | `rg '\{row\.(status\|level\|phase\|type)\}' src`；每个枚举列都要有 label 函数。 |

### A9. 后端族系守卫（AWD/AWDP 页面误调 Jeopardy 端点）

| 项 | 内容 |
|---|---|
| **现象** | AWD/AWDP 赛事工作区里“积分榜/趋势/题目/实例”标签一打开就 400；或未参赛/未开始深链直接渲染正式工作区。 |
| **根因** | 这些标签调用了 Jeopardy 专属端点（后端 `require_jeopardy_family` 会拒绝）；深链闸门在请求之后才判断。 |
| **解决** | 按 `family` 裁剪可用标签（AWD/AWDP 只保留总览/公告等）；深链闸门**先于**请求判断，未参赛/未开始/练习赛给出明确引导。 |
| **规避·检测** | 路由入口先查 `event.family` / `participationGate`；`rg 'scoreboard\|trend\|challenges'` 确认 family 条件。 |

### A10. 管理端写操作“假成功”

| 项 | 内容 |
|---|---|
| **现象** | toast 显示“保存成功”，刷新后数据没变；或“删除 0 条”。 |
| **根因** | mock 未实现写接口却返回成功；或 DELETE body 键名与 SDK 不一致（`id_list` vs `user_id_list`）；或实现只改了静态返回、没改状态。 |
| **解决** | ① mock 未实现的**写**接口统一 501（读接口才返回空数据）；② DELETE/PATCH 按 SDK 实际字段读取；③ 写接口必须有状态，刷新后仍生效。已修：移出成员（`id_list`）、训练 writeup（持久化）、资料保存、设置/角色/武器/题集/实例/Docker/任务。 |
| **规避·检测** | 写操作验证标准：**toast + 列表/详情变化 + 刷新后仍生效**；`rg 'readJsonBody' scripts/mock-api.mjs` 看字段名；mock-check 必须绿。 |

### A11. 管理端页面误调选手端接口（会 401 并清 token）

| 项 | 内容 |
|---|---|
| **现象** | 打开管理端某页后突然被登出；或管理端日志「本人」识别错误。 |
| **根因** | 管理端页面调用了选手端 `/users/me`（管理端 token 的 `sub` 与选手账号不是同一作用域；缺少选手 token 时 401，拦截器清 token）。 |
| **解决** | 管理端一律走 `client.admin.*`；需要“当前管理员身份”时用管理端自己的会话信息/接口，不要复用选手端 `getMe`。 |
| **规避·检测** | `rg 'service\.users\.getMe' src/features/admin` 必须为空；管理端页面在只有 admin token 的环境下逐页打开一次。 |

### A12. 后端分页必须显式带 `page=1`

| 项 | 内容 |
|---|---|
| **现象** | 传了 `limit` 没传 `page`，接口返回全量（分页失效/性能问题）。 |
| **根因** | 后端只有同时收到 `limit` 与 `page` 才分页；缺一个就退回全量。 |
| **解决** | 列表请求统一 `{ page, limit }` 成对出现（首页也要 `page: 1`）。 |
| **规避·检测** | `rg 'limit:' src/features src/api` 时检查同一对象是否有 `page`；抓包确认首页请求带 `page=1`。 |

### A13. React 列表 key 重复

| 项 | 内容 |
|---|---|
| **现象** | 控制台出现 `Encountered two children with the same key`；行操作作用到错误行（如实例列表里同一 GameBox 有多个实例）。 |
| **根因** | key 用了非唯一字段（`gamebox_id`、数组下标、重复 id）。 |
| **解决** | key 用后端唯一标识（`instance.id`，或 `instance.id + gamebox_id` 组合）；排序/筛选后仍保持唯一。 |
| **规避·检测** | 看控制台 warning；`rg 'key=\\{' src` 审查数据源唯一性；列表增删后操作一次行按钮。 |

### A14. SSE 实时页的刷新与输入保留

| 项 | 内容 |
|---|---|
| **现象** | 合法 flag / 分数变化后页面不刷新；或刷新时把正在输入的 flag/补丁内容清空。 |
| **根因** | 只依赖首屏查询；SSE 事件没有按约定失效查询；提交错误后误清空输入。 |
| **解决** | 订阅 SSE（`connected` / `score.updated` 等）后 `invalidateQueries` 对应 `qk`；提交失败保留输入并展示后端错误，成功才清空。 |
| **规避·检测** | 用 `POST /api/__sse/close` 断开重连；提交错误 flag 看输入是否保留；`rg 'EventSource\|score\.updated' src`。 |

---

## 2. 状态、空态与路由（B）

### B1. 空态与“筛选无结果”混用

| 项 | 内容 |
|---|---|
| **现象** | 搜不到时标题仍是「还没有赛事」，或反过来说「还没有正式赛事，去新建」；没有清除入口，用户只能手动清空。 |
| **根因** | `EmptyBlock` 的 `variant` 与标题/描述没有联动。 |
| **解决** | `variant="empty"` 用于“确实没有内容”（附创建入口）；`variant="filtered"` 用于“筛选无结果”（附「清除搜索/筛选」动作 + 具体描述）。已修：公告、题解、解题流水、武器库、管理端赛事。 |
| **规避·检测** | 新列表页两种空态都要造数据验证：清空数据 vs 输入 `zzzz`；检查标题是否具体、动作是否可点。 |

### B2. `PaginationBar` 在 0 条时仍然渲染

| 项 | 内容 |
|---|---|
| **现象** | 空列表下方出现「共 0 条 · 显示 1–0 · 1/1」。 |
| **根因** | 分页组件未对 `total === 0` 短路。 |
| **解决** | `total === 0` 直接返回 `null`。 |
| **规避·检测** | 任何列表都测一次“筛选到 0 条”。 |

### B3. 404 必须留在壳内

| 项 | 内容 |
|---|---|
| **现象** | 输入不存在的地址后整页丢失导航；或管理端 404 跳到选手端。 |
| **根因** | 只有根级 `*`，没有 player shell 的 `*` 与 `admin/*`。 |
| **解决** | `router.tsx` 同时在 player shell 与 admin shell 内注册 404；`NotFoundPage` 在壳内居中（`min-h-[60vh]`），提供返回入口。 |
| **规避·检测** | 手测 `/no-such`、`/admin/no-such`、`/arena/no-such`。 |

### B4. `?tab=` 非法值导致整页空白

| 项 | 内容 |
|---|---|
| **现象** | 工作区链接带着不存在的 tab，内容区空。 |
| **根因** | 直接用 URL 值渲染，没有回退。 |
| **解决** | `availableTabs` 计算 + 非法值回退 `overview`；切 tab 用 `replace`，避免污染浏览历史。 |
| **规避·检测** | 手测 `?tab=not-exist`；路由参数解析处加兜底。 |

### B5. 三态缺失（loading / empty / error）

| 项 | 内容 |
|---|---|
| **现象** | 接口失败只剩空白，用户不知道发生了什么。 |
| **根因** | 直接渲染 `data.items`，没有 `QueryState`。 |
| **解决** | 统一用 `QueryState`（含 skeleton、错误标题、重试）；错误必须可见可恢复。 |
| **规避·检测** | 每个列表页断网/改 mock 返回 500 各跑一次。 |

### B6. 每页条数切换后的页码夹取

| 项 | 内容 |
|---|---|
| **现象** | 在第 3 页把每页 10 条改成 50 条，出现空白页 / 页码越界（如仍显示 `3/1`）。 |
| **根因** | 只改了 `pageSize`，没有把 `page` 重置或夹到有效范围。 |
| **解决** | `onPageSizeChange` 同时 `setPage(1)`；分页组件在 `total` 变化后把页码夹到 `[1, lastPage]`。 |
| **规避·检测** | 末页切换每页条数；筛选后再切换一次。 |

---

## 3. 表单、上传与危险操作（C）

### C1. 原生文件输入泄漏

| 项 | 内容 |
|---|---|
| **现象** | 上传按钮旁出现浏览器原生「Choose File / 未选择文件」。 |
| **根因** | 直接渲染 `<input type="file">`。 |
| **解决** | 隐藏 input（`sr-only`）+ 样式的「选择文件」按钮；保留 `accept` 与键盘可达性。 |
| **规避·检测** | `rg 'type="file"' src` 时确认是不是 `sr-only`；移动端与键盘各测一次。 |

### C2. 上传前缺少客户端校验

| 项 | 内容 |
|---|---|
| **现象** | 选 `.txt` 当头像也能“上传成功”；或超过限制的文件传上去才失败。 |
| **根因** | 只依赖 `accept`（系统文件框可绕过）与后端校验。 |
| **解决** | `onChange` 先校验类型（PNG/JPEG/WebP/GIF）与大小（≤50MB），失败 toast + 不进入上传流程；Writeup 只收 PDF；补丁按后端字段名上传。 |
| **规避·检测** | 用 DataTransfer 造“错误类型 / 超大”文件各测一次；`rg 'accept=' src` 检查配套校验。 |

### C3. 危险操作确认不完整

| 项 | 内容 |
|---|---|
| **现象** | 删除/封禁/切前端/开终端/执行 SQL 只弹一句“确定吗”；或确认按钮无需任何输入即可点。 |
| **根因** | 只用了普通 Dialog，没有后果说明与确认词。 |
| **解决** | `useConfirm({ title, description, consequences[], tone, confirmText, confirmPhrase })`：列出真实后果；高危操作要求输入**对象标识**（学号、角色 key、任务键、`STOP`/`CONNECT`/`EXECUTE`/`OVERRIDE`/`SWITCH`/`delete`）；未匹配时按钮 disabled。 |
| **规避·检测** | `rg 'alert\(\|confirm\(' src` 必须为空；每个破坏性入口都要有 `confirmPhrase` 或明确说明“为什么不需要”（如可撤销）。 |

### C4. 受保护资源仍可被选中删除

| 项 | 内容 |
|---|---|
| **现象** | 选中受保护设置点“删除所选”，确认后报后端 400；内置角色/系统托管赛事也有类似入口；跨页重复挂题（当前页看起来没挂过，实际已挂）。 |
| **根因** | 前端只显示“受保护”标签，没有调整可选/可删状态。 |
| **解决** | 受保护行不可勾选（`isRowSelectable`）或批量删除自动跳过并提示「所选含 N 条受保护设置，删除时会自动跳过」；按钮在无可删项时禁用；重复添加用**全量**已挂载集合判断可选性，而不是当前页。 |
| **规避·检测** | 手测“只选受保护项”“混合选择”两种；`rg 'protected' src/features/admin` 检查交互。 |

### C5. 表单校验与服务端口径不一致

| 项 | 内容 |
|---|---|
| **现象** | 客户端放行、后端拒绝；或两端提示不同（如 Cron、密码长度、邮箱）；一个历史字段未改却阻断其它字段保存；登录空提交只报一个错、焦点不在第一个错误上。 |
| **根因** | 前端只做“非空”，没有按后端规则。 |
| **解决** | 客户端先行校验、服务端兜底：Cron 必须 7 段（秒 分 时 日 月 周 年）；密码 ≥8 且两次一致；邮箱格式；网址/端点格式。错误就地展示（`Field` 的 `error`）；校验只阻断自身字段（历史值视为已接受）；提交时聚焦第一个错误。 |
| **规避·检测** | 每个字段构造“合法/非法”两套输入；`rg 'validation\.' src/lib/locales/zh` 看是否有对应 key。 |

### C6. 操作成功但界面状态没复位

| 项 | 内容 |
|---|---|
| **现象** | 正确 flag 后实例卡仍显示“运行中”；停容器后状态列不变；退出战队后仍显示战队。 |
| **根因** | 只弹 toast，没有失效相关 query（或 mock 没改状态）。 |
| **解决** | mutation `onSuccess` 失效 `qk`（实例/列表/详情/统计）；mock 写接口同步改状态。已修：实例自动回收、容器停止、成员移出、战队退出/加入、资料保存、角色编辑。 |
| **规避·检测** | 每个 mutation 后都检查“页面上的相关数字/状态是否变了”；刷新一次再确认。 |

### C7. 禁用按钮没有原因

| 项 | 内容 |
|---|---|
| **现象** | 「退出战队」「再次训练」「下载源码」等按钮变灰，用户不知道为什么，也没有替代路径；`has_file=false` 的武器仍显示下载链接；实例已停止但端点看起来仍可用。 |
| **根因** | 只传 `disabled`，没有 `title`/说明；隐藏按钮时更没有任何提示。 |
| **解决** | 禁用态统一给 `title`（如“赛事开始后无法退出/更换战队”“只有已结束的 run 可以再次训练”“源码压缩包仅在 Fix 阶段可下载”）；确实隐藏入口时，在页面就近给一句说明与联系管理员的路径；无附件显示「无附件」；停止态明确「端点暂不可用」。 |
| **规避·检测** | 每个 `disabled={...}` 都问一句“用户怎么知道为什么”；`rg 'disabled=\{' src` 抽样检查 `title`。 |

### C8. 登录回跳白名单（开放重定向）

| 项 | 内容 |
|---|---|
| **现象** | `/login?next=//evil.com` 登录后被带到外站；或 `next` 指回登录页造成循环。 |
| **根因** | 直接 `navigate(next)`，没有校验来源。 |
| **解决** | 只接受**站内绝对路径**（以单个 `/` 开头，拒绝 `//`、`http(s)://`、登录页自身）；否则回落默认页。 |
| **规避·检测** | 手测 `next=//evil.com`、`next=https://evil.com`、`next=/login`；`rg 'next=' src/features/auth`。 |

---

## 4. 可访问性、焦点与展示细节（D）

### D1. 跳过链接与主内容区

| 项 | 内容 |
|---|---|
| **现象** | 键盘用户需要 Tab 穿过整个侧栏才能到内容。 |
| **解决** | 壳层提供「跳到主内容」，目标 `#main-content`；`<main id="main-content" tabIndex={-1}>`。 |
| **规避·检测** | 键盘 Tab 一次，确认跳过链接出现且可用。 |

### D2. 弹层关闭后焦点丢失

| 项 | 内容 |
|---|---|
| **现象** | 关闭确认框/抽屉后焦点回到 `body`，键盘用户迷失。 |
| **解决** | `restoreFocusAfterLayer()`：全局记录 `focusin`/`pointerdown`（排除 dialog/menu/tooltip），等待 `data-state !== "closed"` 的层卸载后恢复到触发元素，兜底 `#main-content`；`FormSheet`/`Sheet`/`useConfirm` 关闭时调用。 |
| **规避·检测** | 打开→Esc→确认焦点在触发按钮；`rg 'restoreFocusAfterLayer' src` 检查覆盖。 |

### D3. `SecretValue` 用 CSS 模糊隐藏

| 项 | 内容 |
|---|---|
| **现象** | 界面是模糊的，但 DOM 里仍是明文（复制/读屏可拿到）。 |
| **解决** | 隐藏态渲染固定圆点 `••••••••`，只有“显示”后才把明文放进 DOM；复制走显式动作。 |
| **规避·检测** | 查看元素/DOM 文本，确认隐藏时不存在明文；`rg 'blur' src/components/app`。 |

### D4. 表格选择与批量操作

| 项 | 内容 |
|---|---|
| **现象** | 表头全选把不可选行也算进去；跨页选择丢失；行点击与复选框互相触发。 |
| **解决** | `isRowSelectable` 控制 `row.getCanSelect()` 与 checkbox `disabled`；选择状态由页面 `selectedIds` 受控（跨页保留）；复选框 `onClick` 阻止冒泡；筛选变化时清空不可见选择（按页面语义）。 |
| **规避·检测** | 手测“全选/反选/跨页/筛选后”；可排序表头检查 `aria-sort` 与可访问名称。 |

### D5. 剪贴板“假成功”

| 项 | 内容 |
|---|---|
| **现象** | 复制按钮提示成功，但剪贴板没变（非安全上下文/权限被拒）。 |
| **解决** | 使用统一的 `useCopyToClipboard`，失败时 `toast.error` 并给出可复制的文本/说明。 |
| **规避·检测** | 在 http 非 localhost 与拒绝权限两种情况下各测一次。 |

### D6. 时间与时区

| 项 | 内容 |
|---|---|
| **现象** | `datetime-local` 填写后被当成 UTC；相对时间与绝对时间混用让人误判。 |
| **解决** | 表单加 `LocalTimeHint`（按当前时区填写、保存转 UTC）；列表用相对时间 + `title` 绝对时间；关键时间保留秒。 |
| **规避·检测** | 改一次时区/看一次 `datetime-local` 的保存结果；`rg 'datetime-local' src`。 |

### D7. 裸 UUID / 原始枚举

| 项 | 内容 |
|---|---|
| **现象** | 日志「user 8f1c2d7a-…」整段 UUID；状态列 `running`；日志级别 `INFO`。 |
| **解决** | 当前账号显示「本人」，其它 UUID 截断 + `title` 完整值；枚举走 i18n；空值 `—`。 |
| **规避·检测** | `rg '\{row\.(user_id\|team_id\|status\|level\|type)\}' src`。 |

### D8. 其它易忽略的交互细节

| 项 | 内容 |
|---|---|
| **现象** | 主题切换要点两次才生效；侧栏 `Ctrl+B` 与浏览器/编辑器快捷键冲突；趋势图 hover 位置偏移；表格行内按 Enter 冒泡触发其它动作；移动端行无法选择；训练目录状态与真实阶段不符；移动端缺角色入口/命令面板不可用/行操作重复。 |
| **根因** | 受控状态与 DOM 状态不同步、全局快捷键未判定焦点上下文、图表容器尺寸/缩放未同步、事件未 `stopPropagation`、移动端行没有选择入口。 |
| **解决** | 主题切换只走一次状态更新；全局快捷键先判断 `event.target` 是否在输入/编辑器内，必要时提供可关闭开关；图表 `ResizeObserver` + 正确坐标换算；行内交互阻止冒泡；移动端卡片补复选框；状态文案统一走 label 函数。 |
| **规避·检测** | 键盘 + 鼠标各操作一遍；390px 用触摸/点击各测一次；`rg 'addEventListener\("keydown"' src` 检查是否劫持。 |

---

## 5. 展示口径与 i18n（E）

### E1. 统计数字与列表不同源

| 项 | 内容 |
|---|---|
| **现象** | 概览「用户 128 / 武器 6 / GameBox 6 / 实例 17」，对应列表却是 25 / 3 / 2 / 2；数据大屏「参赛人数 42」而成员页为空；训练 run 页头「我的分数 0」而积分明细「当前总分 100」；讨论 `comment_count=3` 但评论列表 0 条；解题序号出现 `#0`。 |
| **根因** | 统计接口硬编码，或成员/实例等页面缺少数据源。 |
| **解决** | 统计从同一份数据推导（`mockUsers.length`、`weapons.length`、`mockGameboxLibrary.length`、`mockEventInstanceRows.length`、成员列表长度）；同一实体只保留一份计数来源，详情与列表共用同一字段/查询；序号从 1 开始。 |
| **规避·检测** | 逐项对照：概览每个数字 → 找到对应列表页 → 数一遍。 |

### E2. 排序口径与并列规则

| 项 | 内容 |
|---|---|
| **现象** | 排行榜出现 `#6 李明 3 题` 排在 `#7 Kernel 4 题` 前面；同分顺序随机。 |
| **根因** | 直接按原数组顺序编号，没有排序与并列规则。 |
| **解决** | 按后端口径排序后再编号：先 `solved_count` 降序，再 `solved_last_at` 倒序；名次取排序后索引。 |
| **规避·检测** | 造同分数据；检查名次单调性与并列顺序。 |

### E3. 筛选可见性

| 项 | 内容 |
|---|---|
| **现象** | 用户不记得自己筛过什么；筛选后页码不回到 1。 |
| **解决** | `FilterChips` 展示已生效的筛选（可单独移除 + 全部清除）；任何筛选变化 `setPage(1)`；空结果给清除动作。已用于赛事、题库、管理端日志与计划任务等。 |
| **规避·检测** | 每个筛选控件都测“设置→翻页→改筛选”；检查 URL/state 与 chip 一致。 |

### E4. i18n 动态 key 与重复键

| 项 | 内容 |
|---|---|
| **现象** | 某枚举值在英文界面显示中文 key；或 `TS1117` 同一对象重复键导致构建失败。 |
| **根因** | 动态拼接的 key（namespace + 枚举值）没有全部补 key；插入新 key 时与已有同名（如 `filter` 字符串 vs `filters` 对象）。 |
| **解决** | zh/en 同步补 key；插入前 `rg` 同名；`i18n:check` 必须绿（key + 插值占位符）。 |
| **规避·检测** | `node scripts/i18n-check.mjs`；`rg 'TS1117'` 无输出；动态 key 的每个分支都要在两种语言下点一遍。 |

---

## 6. 开发 mock 专项（F）

> mock 只用于开发预览，**不是**真实数据；但 mock 的失真会直接制造“前端 bug”的假象，也会掩盖真实 bug。

### F1. 未实现写接口返回成功 → 假成功

| 项 | 内容 |
|---|---|
| **现象** | 保存成功但刷新没变；删除提示成功但列表不变。 |
| **根因** | 兜底逻辑对所有未匹配请求返回 `data:null`。 |
| **解决** | `/api/*` 未匹配时：**GET 返回 200 空数据**（页面能渲染空态），**非 GET 返回 501**（明确失败）；再用 `mock:check` 追踪覆盖。 |
| **规避·检测** | 新增页面时先跑 `pnpm mock:check`；写操作必须真正改状态。 |

### F2. mock 忽略 `filter` / `page` / `limit`

| 项 | 内容 |
|---|---|
| **现象** | 搜索/筛选没反应；分页标签写「显示 1–10」但表格渲染 25 行；翻页内容不变。 |
| **根因** | 路由直接 `ok(res, rows)`，没有过滤/切片。 |
| **解决** | 实现后端 `filter` 语法（`key:value`，`&` 为 AND、\| 为 OR）；列表用 `okPage(res, rows, req)` 切片并回 `meta.total`。已统一接入讨论、题目、用户、公告、日志、任务、赛事、成员、武器、解题流水等。 |
| **规避·检测** | 每个列表页输入一个必然无结果的词（`zzzz`）与一个必然命中的词；翻到第 2 页；切换每页条数。 |

### F3. 重复路由：先匹配者生效

| 项 | 内容 |
|---|---|
| **现象** | 新写的路由怎么都不生效；接口一直返回旧数据/null；或页面报 500（handler 形参写 `_m` 却取 `m[1]`）；同一份状态被多个赛事复用（E5 显示成 E2 的「进行中」）。 |
| **根因** | 同一 method+path 在 `routes` 里定义了两次，前一条把后一条挡住（曾导致 `/challenges/:id/instance` 恒 null）；handler 参数命名不一致；状态用共享常量而不是按 `eventId` 生成。 |
| **解决** | 新增路由前 `rg` 目标路径；删除/合并重复项；`mock:check` 只校验集合，不校验重复，需人工。 |
| **规避·检测** | 新增前先在 `scripts/mock-api.mjs` 里按方法 + 路径片段 `rg` 一次；改动后用 `curl` 直接打接口确认，而不是只看页面。 |

### F4. mock 状态不持久

| 项 | 内容 |
|---|---|
| **现象** | 改资料/头像/设置/成员后 toast 成功，刷新回滚；写 writeup 后丢失。 |
| **根因** | 返回硬编码对象，没有写入内存状态。 |
| **解决** | 每个写接口维护模块级状态（`mockUsers`/`mockSettings`/`mockChallengeInstances`/`mockRunWriteup`/`mockGameboxLibrary`/`mockDockerContainers`/`mockEventMembers`/`mockChallengeSets` …），读取时从状态生成。 |
| **规避·检测** | 写操作后**刷新页面**仍生效才算通过；mock 重启会重置（预期，文档已注明）。 |

### F5. 协议字段与 SDK 不一致

| 项 | 内容 |
|---|---|
| **现象** | DELETE 移除 0 条；上传 404；SSE 不触发刷新；mock 返回的枚举值与契约不符（`round completed` vs `completed`、`patched` vs `fixed`），页面状态判断全部失灵。 |
| **根因** | DELETE body 键名（SDK `id_list` vs mock `user_id_list`）；multipart 字段（`weapon` / `image_file` / `patch_file` / `writeup_pdf`）；WS 升级未实现。 |
| **解决** | 以 SDK 源码为准实现 mock；上传接口读取 multipart（或至少消费 body 并改状态）；WS 缺失时页面必须显示可见错误与重试。 |
| **规避·检测** | 写接口逐一用 `curl -X DELETE -H 'content-type: application/json' -d ...` 复测；上传用 `-F` 复测。 |

### F6. 静态资源与开发代理

| 项 | 内容 |
|---|---|
| **现象** | 下载 `/public/...` 404；头像“上传成功”但显示首字母；前端选择器「注册表 404」。 |
| **根因** | Vite 只代理了部分前缀；`/public` 返回文本无法解码成图片；`/__floatctf` 未代理。 |
| **解决** | `vite.config.ts` 代理 `/api`、`/public`、`/private`、`/static`、`/__floatctf`；`/public` 对头像返回真实 PNG；`/__floatctf/frontends/registry.json` 提供注册表；注册表缺失时页面 fail-closed（不伪造空注册表）。 |
| **规避·检测** | 手测下载文件、头像显示、前端选择器；`curl -I` 看 content-type。 |

### F7. mock 数据自相矛盾

| 项 | 内容 |
|---|---|
| **现象** | 地址池前缀不合法导致“原值保存也报错”；参赛人数与成员数不一致；实例数 17 与列表 2 条不符；管理端统计 128 与列表 25 不符；积分榜「我」标错、加分加到了榜首（mock 战队 id 与积分行 id 不一致）。 |
| **根因** | 数据手写、各处独立。 |
| **解决** | 同源生成（成员从用户表、实例共用一个数组、统计从数据推导）；配置类数据满足自身校验规则（网络地址必须是网络地址、子网前缀合法）。 |
| **规避·检测** | 打开页面先“原值保存”一次；统计与列表逐项对照；`mock-check` + 手测双保险。 |

### F8. 测试环境限制（不要误判为 bug）

| 项 | 内容 |
|---|---|
| **现象** | Playwright 截图/交互超时；`networkidle` 永远不结束；文件上传被系统选择框拦截；重复的隐藏节点点不到。 |
| **根因** | SSE 长连接、Playwright MCP 与页面不同主机、桌面/移动双份 DOM。 |
| **解决** | 用 `domcontentloaded`；用 `POST /api/__sse/close` 测重连；文件用页面内 `DataTransfer` + `File` 注入（或接口 curl 复测）；选择器加 `:visible`；Radix 下拉用 `[role="option"]`。 |
| **规避·检测** | 测试脚本模板化，避免每次重新踩。 |

---

## 7. 响应式与布局（G）

### G1. 断点与横向溢出

| 项 | 内容 |
|---|---|
| **现象** | 1280 下无问题，390 下横向滚动；KPI 卡片被挤成 1 列；表格列过多撑破容器；宽屏下仍用 3 列导致内容过窄。 |
| **根因** | 固定列数、没有 `min-w-0`/`overflow-x`、隐藏列策略缺失。 |
| **解决** | 卡片网格用 `grid-cols-1 min-[380px]:grid-cols-2 xl:grid-cols-4` 这类渐进断点；表格容器 `overflow-x-auto`；低优先级列 `hideBelow`；移动端用专用卡片（`mobileCard`）；长文本 `break-all` / `line-clamp`。 |
| **规避·检测** | 每条路由 1280 与 390 各扫一次（`document.documentElement.scrollWidth - innerWidth <= 2`）；`rg 'grid-cols-3' src` 检查断点。 |

### G2. 信息架构与品牌（产品向修正，非缺陷）

| 项 | 内容 |
|---|---|
| **现象** | 赛事工作区把报名 / 公告 / 题目 / 实例挤在一页，390px 下拥挤；品牌位置是占位图标，与平台身份不符。 |
| **解决** | 工作区按 `availableTabs` 重组（总览 / 赛制专属 / 公告…），移动端用卡片与底部留白；品牌使用官方 FloatCTF 标志（`BrandMark`）。 |
| **规避·检测** | 1280/390 目测信息密度；`rg 'BrandMark\|logo' src/app`；新增模块先判断“属于哪个 tab”，而不是继续往总览堆。 |

---

## 8. 主题与构建（H）

### H1. 主题 token 与构建门禁

| 项 | 内容 |
|---|---|
| **现象** | 自定义颜色/硬编码色值；暗色下语义色不可读；`pnpm build` 失败在主题审计。 |
| **根因** | 绕过 `docs/theme.md` 的语义色与 CSS 变量。 |
| **解决** | 只用语义 token（`bg-background`、`text-muted-foreground`、`border`、`destructive/10` 等）；图标只用 lucide；`tnum` 用于数字；构建包含 `theme-audit`。 |
| **规避·检测** | `pnpm --filter @floatctf/frontend-shadcn build`（含 `i18n:check` 与 `theme:audit`）；`rg '#[0-9a-fA-F]{6}' src` 人工确认。 |

### H2. 前端制品元数据与引导（安装/激活才暴露）

| 项 | 内容 |
|---|---|
| **现象** | 安装/激活被平台拒绝：`apiContract` 填 `"1"`；或页面加载后 i18n 全部显示 key（资源结构不对）。 |
| **根因** | 制品 `floatctf.frontend.json` 必须声明 `frontendRuntime: "1"`、`apiContract: "2"`（随平台契约升级）；i18n 资源需以 `{ translation: {...} }` 形式注册。 |
| **解决** | 按 `docs/frontend/ARTIFACT.md` 与当前 SDK 契约填写；`i18n.addResourceBundle(lng, "translation", resources[lng])`；版本变更递增 `version`。 |
| **规避·检测** | `scripts/frontend.sh verify <包>`；检查 `dist/floatctf.frontend.json`；`pnpm i18n:check`；安装后在 `?frontend=default` 下对比语言切换。 |

---

## 8A. 真实 API 验收新增问题（第 30–50 轮）

> 这一节是切到真实 dev API 后新增的坑：mock 阶段要么返回了“好看”的数据，
> 要么后端根本不会做这些校验。**mock 绿 ≠ 真实 API 绿**。
> 下面仍按“现象 → 根因 → 解决 → 规避/检测”记录。

### R1. SDK 返回类型比运行时乐观：`data:null` 被 `call` 当成违约

| 项 | 内容 |
|---|---|
| **现象** | 新建题集后数据库里已经有行，页面却 toast「创建题集没有返回数据」，列表不刷新、也不跳详情。 |
| **根因** | SDK 把 `create` 标成 `UniResponse<ChallengeSets>`，真实 API 返回 `{code:0,data:null}`；`call()` 见到 `undefined` 按契约违约抛错。 |
| **解决** | 题集 create / patch 改 `callVoid`；创建成功后用 `filter: name:<name>` 回查（按更新时间倒序取最新）再 `navigate('/admin/challenge-sets/:id')`；仍保留失败 toast。 |
| **注意点** | SDK 类型只是编译期声明，不是运行时保证；写操作不需要返回体就不要 `call<T>`；需要返回体时先看后端 handler 是否真的 `ok(Some(...))`。 |
| **规避·检测** | `rg 'call<null>' src` 必须为空；新 mutation 先看 SDK 签名 + 后端 handler；用真实 API 建一条再刷新；浏览器 Network 看 `data` 是否为 `null`。 |

**相关文件**：`src/features/admin/core/challenge-sets-page.tsx`

### R2. 文件“类型/后缀对了”不等于文件有效

真实 API 对部分上传只做很浅的校验（头像甚至只存字节），前端只看 MIME / 后缀会制造“成功但坏文件”。

| 入口 | 原问题 | 现在的前端校验 | 关键点 |
|---|---|---|---|
| 头像 `profile/pages.tsx` | 8 字节坏 PNG 通过 MIME/大小校验，后端提示「头像已更新」，浏览器却只能显示占位符 | MIME + ≤50MB + `new Image()` 真实解码；失败 toast、撤销 object URL、不进入待上传 | 解码失败不能只靠 `img.onerror` 静默；要清掉已选文件 |
| Markdown 插图 `components/app/markdown.tsx` | `accept=image/*`，坏图上传成功，正文留下永久裂图 | `file.type.startsWith("image/")` + `new Image()` 解码；失败不插入 Markdown | 讨论区、Writeup 等所有 MarkdownEditor 共用 |
| 赛事 Writeup PDF `events/components.tsx` | 文本改名 `fake.pdf` 能选中，提交后才收到后端英文错误 | 先看 MIME/后缀，再读前 5 字节，**精确等于** `%PDF-`；失败中文 toast、不进入待提交 | 不能写 `head.includes("%PDF-")`；后端只认开头 5 字节 |
| 题目包 / GameBox 包 `admin/core/challenges-page.tsx`、`gameboxes-page.tsx` | 文本改名 `.zip` 能进入待导入 | 前 4 字节要求 `PK` + `03/05/07`；失败不设置待导入文件 | ZIP 空归档/分卷分别是 `PK\x05\x06` / `PK\x07\x08` |
| AWDP 补丁 `training/components.tsx`、`awdp/components.tsx` | 文本改名 `.tar.gz` 能选中，后端才报格式错 | 后缀 `.tar.gz/.tgz` + 前 2 字节 `1f 8b`；失败不发起上传 | gzip 头通过不代表里面一定是 tar；后端仍是最终权威 |

**共同注意点**：

- 客户端校验是**提前反馈**，不是安全边界；后端仍必须校验。
- 魔数要按后端真实契约做**精确**比较，不能“包含”或只看扩展名。
- 空文件、超短文件、大小写后缀都要覆盖。
- 校验失败要 `event.target.value = ""`，否则同一个坏文件第二次选不上。
- 解码/预览创建的 object URL 失败时要 `URL.revokeObjectURL`。
- 测试用页面内 `DataTransfer` 注入文件，避免 Playwright 服务与页面不同主机拿不到 `/tmp`。

**检测命令/手法**：

```bash
rg 'accept=|type="file"' src
rg 'new Image\(|%PDF-|0x1f|0x50|accept=' src
# 手工：坏 PNG、文本 .pdf、文本 .zip、文本 .tar.gz 各选一次
```

### R3. 敏感值：列表遮蔽了，编辑区和“解析值”仍是明文

| 项 | 内容 |
|---|---|
| **现象** | 设置列表里 `SMTP_URI` 显示 `••••••••`，点编辑后 Textarea 直接显示完整 SMTP 串；底部「当前解析值」还用 `CopyText` 明文展示。题目静态 flag 列表用 `SecretValue`，编辑 Sheet 却是普通 text input。 |
| **根因** | 遮蔽只做在列表单元格，没有覆盖编辑表单、解析值、详情展示。 |
| **解决** | 设置编辑：敏感键默认 `type=password`，眼睛按钮切换；解析值敏感时用 `SecretValue`。题目静态 flag 编辑同样 password + 眼睛。敏感键判定统一用 `SENSITIVE_KEY_PATTERN`（SECRET/PASSWORD/PASSWD/TOKEN/PRIVATE_KEY/API_KEY/PROXY/SMTP_URI/DATABASE_URL/REDIS_URL）。 |
| **注意点** | 默认隐藏、显式显示；不要用 CSS 模糊（DOM 里仍是明文）；复制也要走显式动作；密码输入加 `autoComplete="off"`、`spellCheck={false}`。 |
| **规避·检测** | 打开每个敏感编辑 Sheet，检查 DOM 里是否有明文；`rg 'isSensitiveKey|SecretValue|type="password"' src`；列表与编辑用同一判定。 |

**相关文件**：`admin/platform/settings.tsx`、`admin/core/challenges-page.tsx`、`components/app/page.tsx`

### R4. AWD/AWDP 生命周期：前端按钮必须跟后端状态机，不跟“配置状态”猜

| 项 | 内容 |
|---|---|
| **现象** | AWD 部署失败后状态仍显示旧值、部署按钮仍可点；手动提前开赛时选手端仍按「赛事未开始」拦住竞技场；赛事页仍显示「未开始」并允许加入/换战队。 |
| **根因** | ① 部署 mutation 失败后只 toast，没有刷新 status；② 前端用 `is_configurable()` 推断可部署，但真实迁移只允许 `Configuring → Deploying`；③ 选手端只看赛事时间窗，没有探测 AWD status / AWDP `started_at`。 |
| **解决** | ① 生命周期 mutation `onError` 调 `refreshAll()`，失败后显示 `deploy_failed`；② 部署只开放 `Configuring`；预检开放 `Configuring/Deployed/Prechecking/Verified/StartBlocked/VerificationFailed`；`DeployFailed/VerificationFailed` 不再显示部署按钮并给恢复说明；③ 选手端先探测 AWD status（`running/paused/network_error` 算 live）和 AWDP overview（`started_at != null` 算 live），覆盖赛事 `upcoming`；④ 提前开赛显示「已提前开赛」并锁 JoinLeaveControl / TeamPanel。 |
| **注意点** | `is_configurable()` ≠ 当前状态可迁移；不要前端自造生命周期；失败路径也要更新 UI；提前开赛不是“赛事状态变成 ongoing”，不能改赛事时间窗来迁就。 |
| **规避·检测** | 造“部署失败”路径看状态是否变；手动 start 后看选手 `/arena/:id`、`/lab/:id`、赛事页是否可进；提前开赛看报名/换队是否锁；`rg 'DEPLOYABLE_STATUSES|PRECHECKABLE_STATUSES|liveBeforeWindow|earlyStart' src`。 |

**相关文件**：`admin/awd-ops/shared.tsx`、`admin/awd-ops/pages.tsx`、`awd/pages.tsx`、`awdp/pages.tsx`、`events/pages.tsx`、`events/components.tsx`

### R5. AWDP 补丁格式：后端只收 `patch.tar.gz`（根目录 `patch.sh`）

| 项 | 内容 |
|---|---|
| **现象** | 页面接受 `.sh` / 任意文件，提交后后端拒绝；或选 `.tar.gz` 但里面不是 tar，直到上传后才报错。 |
| **根因** | UI 的 `accept` 与文案按“脚本文件”设计，和真实契约不一致；只校验后缀。 |
| **解决** | 训练场与赛事 AWDP 统一 `accept=".tar.gz,.tgz,application/gzip"`；扩展名校验 + gzip 魔数；文案明确「patch.tar.gz / 根目录 patch.sh」。 |
| **注意点** | 不要在前端解包/假装校验 tar 内容；gzip 头只挡明显错误文件；最终仍由后端判定。 |
| **规避·检测** | 用文本 `.tar.gz`、真实 tar.gz 各测一次；`rg 'patchFormat|patchGzip|accept=' src/features/{training,awdp}`。 |

### R6. 练习赛没有官方数据大屏：不要对 `purpose=practice` 调正式积分接口

| 项 | 内容 |
|---|---|
| **现象** | 打开练习赛的「数据」标签，后端 400 `UnsupportedForPurpose: practice has no official event scoreboard`。 |
| **根因** | 前端只按 `family=Jeopardy` 显示数据标签，没有按 `purpose` 裁剪；正式赛/练习赛能力不同。 |
| **解决** | 数据标签只在 `purpose == competition` 时出现；练习赛回退到配置/总览，不请求正式积分端点。 |
| **注意点** | 能力判定要同时看 family + purpose + participant mode；空数据不是错误，不能把 400 当“暂无数据”。 |
| **规避·检测** | `rg 'purpose.*practice|UnsupportedForPurpose|dataTab' src`；用练习赛和正式赛各打开一次工作区。 |

### R7. 中文题名不能手工创建：后端要求 ASCII `safe_name`

| 项 | 内容 |
|---|---|
| **现象** | 手工新建「中文题名」提交后后端返回 `CHALLENGE_SAFE_NAME_REQUIRED`。 |
| **根因** | 手工 create 只接受能派生 ASCII `safe_name` 的名称；中文名必须走题目包并在 `meta.toml` 显式给 `safe_name`。 |
| **解决** | 前端校验名称至少含 ASCII 字母/数字；提示中文名请导入题目包并设置 `safe_name`；错误就地展示。 |
| **注意点** | 不要在前端“音译/随机生成” safe_name；不要让用户提交后才看到后端错误。 |
| **规避·检测** | 手工建中文名看是否被前端拦住；导入包含中文名 + 显式 safe_name 的包看是否成功。 |

### R8. 单点趋势图重复 key 与 epoch 0 时间

| 项 | 内容 |
|---|---|
| **现象** | 只有一个时间点时控制台报 duplicate key；缺失时间显示 `1970/01/01`。 |
| **根因** | 图表 x 轴把 min/max 相同的时间生成两次 tick；`AbsoluteTime` 把 `0` 当合法 epoch。 |
| **解决** | `xTickValues = maxX === minX ? [minX] : [...]`；`AbsoluteTime` 对 `0/null` 显示 `—`，并抑制 1970 title tooltip。 |
| **注意点** | 图表空/单点/多点都要造；时间戳 `0` 往往表示“未设置”，不是 Unix epoch。 |
| **规避·检测** | 造单点趋势；看控制台 key warning；`rg 'xTickValues|epoch|1970' src`。 |

### R9. 非法 UUID 深链接：后端 400，不是 404

| 项 | 内容 |
|---|---|
| **现象** | `/admin/events/does-not-exist`、`/arena/does-not-exist` 显示「路径参数格式错误」+ 重试，而不是资源不存在；AWD 运维页甚至没有 not-found 分支。 |
| **根因** | 路由参数不是合法 UUID，后端返回 400「路径参数格式错误」；前端只把 404 当 not-found。 |
| **解决** | `isNotFound` 对 HTTP 400 且 platformMessage 含「路径参数 / path parameter」也返回 true；AWD 运维补 NotFoundBlock + 返回入口；赛事/选手/AWDP/训练等复用统一判定。 |
| **注意点** | 不要把所有 400 都当 not-found，只匹配路径参数错误；非法 ID 不应触发重试（重试也不会成功）；每个 shell 都要有壳内 404。 |
| **规避·检测** | 手测 `/admin/events/x`、`/admin/events/x/awd`、`/events/x`、`/arena/x`、`/lab/x`、`/training/runs/x`；`rg 'isNotFound' src`。 |

### R10. 后端契约问题：前端如实展示，不要用 UI 掩盖

这些是在真实 API 验收中发现、但**根因不在前端**的问题；已记录，未擅自改后端。

| 问题 | 现象/证据 | 处理建议 |
|---|---|---|
| SQL 控制台 INT2/INT4 变 NULL | `select 1::int4` 返回 `{"i4":null}`；INT8/FLOAT8/BOOL 正常 | 后端 `rows_to_json` 按 INT2→i16、INT4→i32、INT8→i64 解码；前端不要伪造值 |
| `DeployFailed` 可配置但不能直接重部署 | 状态机只允许 `DeployFailed → Configuring`，前端若直接给“部署”会失败 | 前端只给“重新配置”路径；需要后端明确恢复动作 |
| 部署后新建战队可能绕过 GameBox/网络预检 | 预检通过但新队无资源，running 后不能重部署 | 后端在预检/部署时校验全部战队；前端不假装已就绪 |
| 删除赛事后可能留下定时任务 | 管理端任务列表出现已删赛事的 `awd.event.hardening_end` 失败任务 | 后端清理/标记；前端不要把它当当前赛事状态 |

**原则**：前端可以展示后端错误、禁用不可行动作、给恢复引导，但不能把 null 改成 0、把失败改成成功、把未知状态猜成成功。

### R11. 真实 dev 验收操作要点

- 真实栈：API `:9090`、Caddy `:7780`、Default Vite `:13000`、shadcn Vite `:13400`；浏览器访问 `http://10.66.66.2:13400`，不要用 `127.0.0.1`（Playwright 与服务不同主机）。
- 用普通 `pnpm`，不要依赖 `mise exec -- pnpm`（工具缺失时会失败）。
- 等页面用 `domcontentloaded`，不要等 `networkidle`（SSE 长连接永远不 idle）。
- 文件上传用页面内 `DataTransfer` + `File` 注入；接口层再用 `curl -F` 复核。
- 写操作验收标准：toast + 列表/详情变化 + **刷新后仍生效**；失败路径有可见错误。
- 测试数据清理：用户/角色/赛事/题目/题集/讨论/武器用 UI 删除或 SQL 清理；`test-g` GameBox 与训练 run fixture 保留。
- 前端门禁：`tsc --noEmit`、`vite build`、`node scripts/i18n-check.mjs`、`node scripts/theme-audit.mjs`、`node scripts/mock-check.mjs`。

### R12. 题目详情：外层 5:7 分栏把工作台描述压成竖排

| 项 | 内容 |
|---|---|
| **现象** | 宽屏题目详情中，左侧「题目描述」被压成极窄一列，中文逐字竖排；右侧出现大块空白，页面看起来像坏掉。 |
| **根因** | 题目详情外层使用了 Default 的 `5fr : 7fr`（左题目、右我的题解）；`ChallengeWorkbench` 内部又使用 `minmax(0,1fr) + 360px`（左描述、右环境/Flag）。左栏实际只剩 ~450px，360px 侧栏抢走后描述仅 ~90px。 |
| **修复** | 工作台改为单列：描述 → 解题环境 → 提交 Flag → 可选 Writeup 区块纵向排列；去掉内部固定 360px 侧栏。外层仍保留 Default 的 5:7（左题目、右常驻题解）。 |
| **规避·检测** | 宽屏（≥1920）和 1280 各打开一次题目详情；检查描述区宽度 ≥ 400px，不能出现逐字竖排；同时扫描 `grid-cols-[...360px]`，避免再把固定侧栏塞进窄容器。 |

### R13. 题集内题目应弹窗，不是题集名称弹窗

| 项 | 内容 |
|---|---|
| **现象** | 起初把“题集里点题目弹窗”误改成“点题集名称弹窗”，导致题集名称不再进入详情。 |
| **根因** | 对 Default 的交互层级理解错位。Default：题集名称 → 题集详情页；详情里的题目名称 → 题目弹窗。 |
| **修复** | 题集名称恢复为详情页链接；在题集详情中点击题目名称打开 Dialog，内部加载题目详情并复用 `ChallengeWorkbench`（描述 / 环境 / Flag），题集内不跳题。 |
| **规避·检测** | 点击题集名称断言进入 `/challenge-sets/:id`；点击题目名称断言 URL 不变且 `[role=dialog]` 可见；桌面 1280 与移动 390 检查弹窗宽度与横向滚动。 |

### R14. 空状态贴边 + 题集题目弹窗打开时标题被滚走

| 项 | 内容 |
|---|---|
| **现象** | 列表空态虚线框紧贴卡片边缘，空列表仍显示分页；题集题目弹窗打开时自动滚到中部，标题/弹窗头像不可见。 |
| **根因** | 空态直接放在 `contentClassName="px-0"` 的卡片内容里；题集弹窗用单个滚动容器，Radix 自动聚焦首个小控件（启动/Flag）导致滚动。 |
| **修复** | `EmptyBlock` 在卡片内统一保留 16px 外边距；空列表不渲染分页；题目弹窗改为「固定标题头 + 正文独立滚动」，`ChallengeWorkbench` 在弹窗内禁用自身滚动。 |
| **规避·检测** | 空列表检查 dashed 边框与卡片边缘间距；弹窗打开后断言标题在视口内且 `scrollTop=0`；正文可滚动到提交 Flag。 |

### R15. 赛事列表卡片头部重复“每页”选择，单页仍显示分页脚

| 项 | 内容 |
|---|---|
| **现象** | 赛事列表卡片头部有一个“20/页”，底部分页条又有一个“每页 20 条”；4 条数据仍出现整条分页脚，筛选条被挤到下一行。 |
| **根因** | 头部 Select 与 `PaginationBar` 重复提供 pageSize；筛选条 `w-full` 在工具条中换行。 |
| **修复** | 删除头部重复的每页选择，只保留分页脚；单页数据不渲染分页脚；筛选条占满剩余宽度。 |
| **规避·检测** | 列表只有一页时不应出现分页脚；卡片头只允许一个 pageSize 控件；检查筛选条与其它工具按钮的 y 坐标一致。 |

---

## 9. 提交前检查清单（可直接照做）

### 9.1 命令门禁

```bash
cd frontends/floatctf-frontend-shadcn
pnpm typecheck                       # tsc --noEmit
pnpm build                           # vite build + tsc + i18n + theme
pnpm mock:check                      # 路由快照 + 关键端点覆盖
# 有意新增 mock 路由后：
pnpm mock:check -- --update          # 并提交 scripts/mock-routes.snapshot.json
```

### 9.2 代码检索（评审时 1 分钟）

```bash
rg 'call<null>' src                                  # 必须为空
rg 'alert\(|confirm\(' src                          # 必须为空（用 useConfirm）
rg '\{row\.(status|level|phase|type)\}' src           # 直接渲染枚举
rg 'type="file"' src                                  # 是否有配套隐藏/校验
rg 'new Image\(|%PDF-|0x1f|0x50' src                # 图片解码 / 文件魔数校验
rg 'isSensitiveKey|type="password"|SecretValue' src    # 敏感值是否全链路遮蔽
rg 'isNotFound|NotFoundBlock' src                     # 404 / 非法 UUID 是否有壳内兜底
rg 'DEPLOYABLE_STATUSES|liveBeforeWindow|earlyStart' src # 状态机 / 提前开赛
rg 'solved_percent|formatPercent' src                 # 百分比口径
rg 'info\.id\b|EventInfo' src                         # 是否误用顶层 id
rg 'filter:' src                                      # 筛选是否真的传给后端
rg 'datetime-local' src                               # 是否有 LocalTimeHint
rg 'next=' src                                        # 登录回跳是否白名单
rg 'disabled=\{' src                                  # 禁用是否有原因
```

### 9.3 浏览器手测（每个页面 2 分钟）

1. 1280 与 390 各开一次：无横向滚动、无白屏、无「加载失败」。
2. 列表：搜索一个命中词、一个 `zzzz`；翻到第 2 页；切换每页条数；确认计数与行数一致。
3. 写操作：成功 toast、列表变化、**刷新后仍生效**；失败路径有可见错误。
4. 危险操作：确认框有真实后果；需要确认词的必须输入正确才可点；取消后无副作用。
5. 弹层：Esc 关闭后焦点回到触发元素；Tab 能到达“跳到主内容”。
6. SSE 页面：用 `domcontentloaded` 等待；断线后应自动重连/降级轮询且提示可见。

### 9.4 真实 API 联调清单（切 mock 后必做）

1. 先确认栈：API `:9090`、Caddy `:7780`、shadcn Vite `:13400`；浏览器用
   `http://10.66.66.2:13400`，Playwright 不要用 `127.0.0.1`。
2. 写操作看 Network：`data:null` 用 `callVoid`；列表刷新后仍在才算成功。
3. 文件上传先造坏文件：坏 PNG、文本 `.pdf`、文本 `.zip`、文本 `.tar.gz`；
   前端应在选择阶段拦截，而不是提交后才报后端英文错误。
4. 敏感编辑：打开设置 `SMTP_URI`、题目静态 flag 编辑，检查 DOM 中默认没有明文；
   解析值 / 关联展示同样遮蔽。
5. 生命周期 / 提前开赛：部署失败后状态要变；手动 start 后选手 `/arena`、`/lab`、
   赛事页要能正确进入/锁定。
6. 非法 UUID：`/admin/events/x`、`/arena/x`、`/training/runs/x` 应显示壳内 not-found，
   不是“路径参数格式错误 + 重试”。
7. 清理测试数据；保留 `test-g` / 训练 fixture 时在交付说明中写清楚。

---

## 10. 已知边界（本轮未闭环，不要误以为已验证）

1. **WebSocket 终端**：mock 无 WS，端到端 shell 未验证；已验证“确认词 → 票据 → 失败可见 → 重试”的失败路径。
2. **AWD/AWDP 管理端 PATCH 的乐观锁/冲突提示**：未系统验证（接口存在，冲突分支未造数据）。
3. **真实 API 回归**：后半程已切到真实 dev 栈（API `:9090` / Caddy `:7780` / shadcn Vite `:13400`），
   覆盖题集创建、用户/角色/公告、事件、题目包导入、AWD/AWDP 生命周期、训练、讨论、
   武器、资料、设置、非法路由与移动端。完整逐轮证据见
   `frontends/floatctf-frontend-shadcn/docs/qa/AUDIT-50.md`。
4. **移动端全量回归**：已对主要路由做 320×568 / 390×844 两档扫描（无横向溢出、无 console error）；
   表格行操作、命令面板、超长文本的极端组合仍未逐一穷举。
5. **mock 数据**：仅开发预览，重启即重置；`MOCK_AWD_ADMIN_STATUS`、`MOCK_AWDP_ADMIN_PHASE`、
   `MOCK_TEAM_BANNED`、`MOCK_RUN_STATE` 等开关用于构造边界状态。
6. **SQL 控制台 INT2/INT4**：后端 `rows_to_json` 当前返回 NULL（INT8/FLOAT8/BOOL 正常）；
   已在 §8A.R10 记录，未擅自改后端。
7. **AWD 攻击正确 flag 的跨队计分**：单队 fixture 下未完整覆盖；真实跨队攻防仍需多队环境。
8. **测试 fixture**：`test-g` GameBox 与训练 run 保留；测试用户/角色/赛事/题目/题集/讨论/武器已清理。

---

## 11. 开发过程与工具链（目标执行期间实际踩坑）

> 这一节不是产品缺陷，而是“做这 50 轮时把自己坑住”的问题。它们会直接拖慢下一次联调/回归，
> 有些还会造成**代码或 mock 数据丢失**。

### I1. 子模块拉取失败

| 项 | 内容 |
|---|---|
| **现象** | `git submodule update` 报 GnuTLS/网络错误，前端目录为空。 |
| **解决** | 重试；必要时 `git submodule sync` 后单独 clone；不要改业务代码。 |
| **规避·检测** | 先确认网络与远端可达；失败信息里区分 TLS 与认证错误。 |

### I2. 前端制品契约版本被拒

| 项 | 内容 |
|---|---|
| **现象** | 安装/激活时平台拒绝：`apiContract` 填 `"1"`。 |
| **解决** | 制品 `floatctf.frontend.json` 用 `frontendRuntime: "1"`、`apiContract: "2"`；随平台契约升级同步。 |
| **规避·检测** | `scripts/frontend.sh verify`；对照 `docs/frontend/ARTIFACT.md`。详见 H2。 |

### I3. i18n 资源结构不对

| 项 | 内容 |
|---|---|
| **现象** | 页面显示原始 key（如 `events.list.title`）。 |
| **解决** | 资源按 `{ translation: { ... } }` 注册（`i18n.addResourceBundle(lng, "translation", resources[lng])`）。 |
| **规避·检测** | 构建后切中英各看一页；`i18n:check`；详见 E4/H2。 |

### I4. 用 `pkill -f` 停服务，把自己的 shell 也杀了

| 项 | 内容 |
|---|---|
| **现象** | 执行 `pkill -f node`（或类似宽匹配）后命令会话直接断开，后续命令全失败。 |
| **解决** | 按端口找 PID：`ss -ltnp \| grep ':7780'`，再 `kill -9 <pid>`；或 `fuser -k 7780/tcp`。 |
| **规避·检测** | 禁止宽匹配 `pkill -f`；停服务前先打印 PID 并确认命令行。 |

### I5. 批量字符串替换把 mock 路由改没了（事故）

| 项 | 内容 |
|---|---|
| **现象** | 一次分块替换后 `node --check` 通过，但页面大量 404/空白；对比发现 **24 条选手端路由消失**。 |
| **根因** | 分块替换的起止边界算错，把相邻路由一起吞掉；单次改动过大。 |
| **解决** | 用 `git show <commit>:scripts/mock-api.mjs` 恢复被删段落；补 `mock:check` 路由快照 + 关键端点覆盖，之后同类问题能被 CI 拦住。 |
| **规避·检测** | ① 每次只改一小块，改完立刻 `node --check`；② 跑 `pnpm mock:check`（路由数 + 关键端点）；③ `git diff` 人工看被删行；④ 大改前先提交/备份。 |

### I6. `git checkout --` 覆盖未提交工作（事故）

| 项 | 内容 |
|---|---|
| **现象** | 改 mock 出现语法错误后执行 `git checkout -- scripts/mock-api.mjs`，把**未提交的 18 条管理端 AWD 路由**一起回滚；14 轮后才发现。 |
| **根因** | 用整文件 checkout 处理局部语法错误，且当时那批改动没有提交。 |
| **解决** | 按功能重新补齐（teams/scores/prechecks/network/gameboxes/instances/lifecycle/tokens/ban 等）。 |
| **规避·检测** | 出现语法错误时：先 `node --check` 定位、局部编辑；必须回滚时用 `git diff` 备份或 `git stash`；**禁止**对含未提交改动的文件做整文件 checkout。 |

### I7. 重复路由：先匹配者生效

| 项 | 内容 |
|---|---|
| **现象** | 新写的 mock 路由不生效，接口一直返回旧值/`null`（出现过两次：`/challenges/:id/instance`、`/instances/...` 组）。 |
| **根因** | 同一 method+path 定义了两次，数组顺序靠前的那条把后一条挡住。 |
| **解决** | 删除/合并重复定义；新增前按路径 `rg` 一次；`mock:check` 不查重复，需人工。 |
| **规避·检测** | 改完用 `curl` 直接打接口，而不是只看页面；`rg` 同名路径。详见 F3。 |

### I8. 忘记更新 mock 路由快照

| 项 | 内容 |
|---|---|
| **现象** | 加了路由后 `mock:check` 报“新增 N 条”，CI 红；或提交后发现 `mock-routes.snapshot.json` 没进 commit。 |
| **解决** | `pnpm mock:check -- --update`，把 `scripts/mock-routes.snapshot.json` 一并提交。 |
| **规避·检测** | 加/删 mock 路由后立刻 update；提交前再跑一次 `mock:check`。 |

### I9. Playwright/浏览器自动化陷阱

| 项 | 内容 |
|---|---|
| **现象** | 等待 `networkidle` 永不结束；单次脚本路由太多报 30s 超时；传文件报 `ENOENT`；页面里 `Buffer is not defined`；隐藏的重复按钮点不到；Radix 下拉选不中；确认框按钮一直 disabled；用 `isEnabled()` 判断状态时超时；弹层挡住了后续点击。 |
| **根因** | SSE 长连接让 networkidle 不成立；MCP 有单次执行时限；Playwright 服务与页面不同主机，拿不到本地 `/tmp`；页面上下文没有 Node `Buffer`；桌面表格与移动卡片是**两套 DOM**；Radix 选项不是原生 option；确认词未输入时按钮 disabled；上一轮弹层未关闭。 |
| **解决** | 用 `domcontentloaded` + 显式 `waitForTimeout`/条件等待；把长扫描拆成 ≤20 路由/批；文件用页面内 `DataTransfer` + `File`（或 canvas 生成 PNG）注入，接口层用 `curl -F` 复测；选择器加 `:visible`；下拉用 `[role="option"]`；确认框先填 `#confirm-phrase`；先按 `Escape` 关闭残留弹层；判断按钮状态用 `count()`/`getAttribute("disabled")`，不要直接 `isEnabled()`。 |
| **规避·检测** | 固化测试脚本模板；页面断言区分“正常 404 / 筛选空态 / 错误边界”，避免把 404 文案当崩溃。 |

### I10. 测试状态与 token 丢失

| 项 | 内容 |
|---|---|
| **现象** | mock 重启后成员/头像/设置/写操作全部回默认；浏览器重启后页面 401；SSE 仍连着旧进程。 |
| **根因** | mock 状态在内存；token 存在 `localStorage`；SSE 长连接。 |
| **解决** | 测试前重建数据；`localStorage.setItem('shadcn.user.token', ...)`（管理端另有 `shadcn.admin.token`）；必要时 `POST /api/__sse/close` 后重连。 |
| **规避·检测** | 写操作验证必须“刷新后仍生效”；mock 重启视为环境重置。 |

### I11. 编辑 locale 文件反复踩的坑

| 项 | 内容 |
|---|---|
| **现象** | `TS1117` 同一对象重复键；新 key 插到了错误层级；动态 key 漏翻译但 `i18n-check` 不报。 |
| **根因** | locale 对象里有同名短键（`filter` 字符串 vs `filters` 对象）；按缩进做字符串插入不稳定；`t(\`ns.${value}\`)` 是运行期拼接，静态检查覆盖不到。 |
| **解决** | 插入前 `rg '^\t+key:'` 查重；用唯一锚点（完整一行）替换；动态 key 的每个取值手工补齐 zh/en。 |
| **规避·检测** | 每次改完 `tsc --noEmit` + `pnpm i18n:check`；两种语言各点一遍相关页面。 |

### I12. 父仓库未提交状态

| 项 | 内容 |
|---|---|
| **现象** | 交付时父仓库仍有 submodule 指针、`pnpm-lock.yaml`、未跟踪的 `docs/theme.md`。 |
| **解决** | 交付说明里列清楚哪些是“有意未提交”，或按需分批提交；不要误 `git add -A`。 |
| **规避·检测** | 提交前 `git status --short`；只 `git add` 目标文件。 |

### I13. 文档/README 与代码漂移

| 项 | 内容 |
|---|---|
| **现象** | README 的“契约缺口”仍写着 `weapons.fetch` 忽略参数且无 meta，实际后端已支持 `filter` 与分页 meta。 |
| **解决** | 以源码为准更正 README；交付前对照 `packages/sdk` 与后端 handler 再写契约说明。 |
| **规避·检测** | 改动公共契约时同步更新 README/文档；评审时用 `rg` 对照。 |

### I14. 全站扫描的断言误报

| 项 | 内容 |
|---|---|
| **现象** | 路由扫描把 `/scoreboard`（该前端本就没有此路由）的 404 页面当成“崩溃”；把筛选空态当成“加载失败”。 |
| **根因** | 断言用词过宽（`页面不存在`、`加载失败` 同时出现在正常态与错误态）。 |
| **解决** | 断言区分：正常 404（壳内）、筛选空态（有清除动作）、错误边界（有重试）；只对目标页面断言“错误提示”。 |
| **规避·检测** | 扫描脚本维护“预期 404”清单；失败时先看截图/文本再判定。 |

### I15. 开发服务器、代理与环境变量

| 项 | 内容 |
|---|---|
| **现象** | 改了 `vite.config.ts` 的代理但页面仍 404；设置了 `MOCK_*` 但行为没变；端口被旧进程占用，新代码不生效。 |
| **根因** | Vite 配置变更需要重启（或等待自动重启完成）；环境变量必须在启动命令前设置；旧的 mock 进程没被杀掉。 |
| **解决** | 改配置后重启 Vite；`MOCK_TEAM_BANNED=1 setsid node scripts/mock-api.mjs ...`；启动前 `ss -ltnp` 确认端口归属，必要时按 PID 杀掉旧进程。 |
| **规避·检测** | 用 `curl` 直连 mock 验证新行为；看启动日志里的监听地址；代理前缀变化时同时更新 Vite 与线上 Caddy 语义。 |

---

## 附 A：与 50 轮台账的对应关系

> 逐轮流水见 [`frontends/floatctf-frontend-shadcn/docs/qa/AUDIT-50.md`](../frontends/floatctf-frontend-shadcn/docs/qa/AUDIT-50.md)。
> 这里按主题归并，方便从“当时那一轮”回查“以后怎么防”。

| 台账范围 | 主题 | 本文对应 |
|---|---|---|
| 1 | 登录 / 注册 / 回跳 | C5、C8 |
| 2 | 选手总览 | A12、E1、E3 |
| 3 | 赛事列表筛选 | E3 |
| 4–9 | 赛事工作区 / 赛制差异 / 深链闸门 | A9、G2 |
| 10–12 | AWD 驾驶舱 / flag 提交 | A1、A7、A8、A14 |
| 13–18 | AWDP 工作台 / 实例 / 赛事内解题 | A1、A13、C2、C6 |
| 19 | 题集 | A1、C4 |
| 20–21 | 训练目录 / run | A8、C6、E1 |
| 22–29 | 排行榜 / 流水 / 社区 / 武器 / 题解 | E2、F2、F6、C7 |
| 30 | 我的资料 | C2、C5、F4 |
| 31–41 | 管理端赛事（配置/成员/战队/公告/实例/日志/Writeup/大屏） | A11、A12、C3、C5、C7、E1、F3 |
| 42–43 | AWD / AWDP 运维 | A7、A8、F4 |
| 44–49 | 用户 / 题库 / GameBox / 角色 / 设置 / Docker / SQL / 终端 | A1、C3、C4、C5、F4、F5、H2 |
| 50 | 404 / 空态 / i18n / 主题 / 构建 | B1、B3、E4、H1 |

> 上表是早期 mock 阶段的主题归并；后半程切真实 API 后新增的 R1–R11 见 §8A。

---

## 附：问题 → 章节索引

| 你会看到的现象 | 先看 |
|---|---|
| 「没有返回数据」/ 写操作必失败 | A1 |
| 白屏 / `Query data cannot be undefined` | A2 |
| 搜索、筛选、翻页没反应 | F2 |
| 统计数字与列表对不上 | E1 |
| 分页标签与表格行数不一致 | F2 |
| 空态文案不对 / 没有清除入口 | B1 |
| 删除后仍可点、确认框太轻 | C3、C4 |
| 上传“成功”但文件坏了 / 头像不显示 | C2、F6 |
| 状态没刷新 / 数字不变 | C6 |
| 排行榜/积分榜顺序或合计不对 | E2、A7 |
| 中文界面出现英文枚举 | A8、E4 |
| 表格 390px 溢出 | G |
| 构建挂在 i18n / 主题审计 | H、E4 |
| 前端选择器“注册表 404” | F6 |
| 改了 mock 但不生效 | F3 |
| 管理端打开后突然被登出 | A11 |
| 列表接口返回全量 / 分页失效 | A12 |
| 按钮变灰但没有说明 | C7 |
| 登录后被带到外站 | C8 |
| 主题切换 / 快捷键 / 图表交互异常 | D8 |
| 列表行操作作用到错行 / React key 警告 | A13 |
| 实时数据不刷新 / 输入被清空 | A14 |
| 写操作已落库却提示“没有返回数据” | R1、A1 |
| 坏图片/假 PDF/假 ZIP 能选中并“成功” | R2、C2 |
| 敏感设置/静态 flag 编辑区明文 | R3、D3 |
| 部署失败状态不刷新 / 提前开赛被拦住 | R4 |
| 练习赛数据标签 400 / 中文题名创建失败 | R6、R7 |
| `/admin/events/x` 显示“路径参数格式错误” | R9 |
| SQL 控制台 INT4 显示 NULL | R10 |
| 末页切换每页条数出现空白页 | B6 |
| 工作区拥挤 / 品牌占位 | G2 |
| 安装被拒 / 界面全是 i18n key | H2 |
| 改了 mock 后大量路由消失 | I5 |
| 语法错误回滚后功能没了 | I6 |
| mock 路由怎么都不生效 | I7 |
| mock:check 报路由数变化 | I8 |
| Playwright 等待/点击/上传异常 | I9 |
| 重启后数据与登录状态丢失 | I10 |
| locale 构建报 TS1117 | I11 |
| 代理/环境变量改了不生效、端口被占 | I15 |
