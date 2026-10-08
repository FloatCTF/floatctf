# FRONTEND-PLAN — FloatCTF Shadcn Console

> 本文件按 [AI-FRONTEND-GUIDE.md](../../docs/frontend/AI-FRONTEND-GUIDE.md) §11 的九段模板编写，
> 在写实现代码**之前**产出，并作为后续覆盖度自查的依据。
> 能力基准：[CAPABILITY-MATRIX.md](../../docs/frontend/CAPABILITY-MATRIX.md)。

## 1. Identity

- Frontend id: `shadcn`（满足 `[a-z0-9][a-z0-9._-]*`，≤64）
- Name / version: FloatCTF Shadcn Console / `0.1.0`
- Scope: **complete**（选手端 + 管理端；覆盖 CAPABILITY-MATRIX 全部 89 项 `required`）
- 目标目录：`frontends/floatctf-frontend-shadcn/`（本仓库内实现，不触碰 `frontends/default/`）
- 语义参照：Default Frontend（只读，用于核对业务能力 / 鉴权语义 / 状态语义 / 错误边界 / 实时行为），
  **不作为视觉、布局、路由、组件或交互模板，也不是依赖**。

## 2. Technology

| 关注点 | 选择 |
|---|---|
| 框架 | React 19 + TypeScript 5.7 |
| 构建 | Vite 6（`base: "./"`，lib 模式，入口产物固定 `assets/frontend.js` + `assets/frontend.css`） |
| 组件库 | **shadcn/ui**（new-york 风格；Radix primitives + CVA + `tailwind-merge`）；图标 `lucide-react` |
| 样式 | Tailwind CSS v4（`@tailwindcss/vite`）+ shadcn CSS 变量主题（暗色优先，`zinc` 基色） |
| 路由 | react-router v7（声明式嵌套路由，自有路由树；深链 + SPA fallback 全部由本前端拥有） |
| 服务端状态 | TanStack Query v5（配合 `@floatctf/react` 的 query 工厂与 SSE hooks） |
| 客户端状态 | Zustand v5（仅 auth / UI 偏好）；token 持久化策略归本前端 |
| 表单 | react-hook-form + zod + shadcn `Form`（管理端 CRUD 统一形态） |
| 表格 | TanStack Table v8 + shadcn `Table`（`DataTable` 封装：排序 / 过滤 / 分页 / 行选择） |
| 通知 / 对话框 | shadcn `Sonner`（toast）+ Radix `Dialog` / `AlertDialog` / `Sheet` / `Drawer` |
| 命令面板 | shadcn `Command`（cmdk），⌘K / Ctrl-K 全局可达 |
| Markdown | `react-markdown` + `remark-gfm`（**不启用** `rehype-raw`，不接受原始 HTML 注入）；编辑器为「Write / Preview」双栏，图片上传走 `uploads.upload_image` |
| 图表 | 自研 SVG（`Sparkline` / `StepArea`），不引入重量级图表库 |
| 终端（specialized） | `@xterm/xterm` + `@xterm/addon-fit`（管理端 Web 终端；用 `client.adminHttp` + 原生 `WebSocket` 逃生舱） |
| 依赖边界 | 只 import `@floatctf/sdk`、`@floatctf/frontend-runtime`、`@floatctf/react` 与第三方库；**禁止** `frontends/default/*`、`apps/web/*`、`packages/*/src/*`、`@/*`（本包自己的内部 alias 用 `~/*` → `./src/*`） |

## 3. Visual direction

- **设计语言**：shadcn/ui 原生语汇 —— 中性灰阶（zinc）、1px 边框、低饱和强调色、`rounded-lg` 卡片、
  无渐变无阴影堆叠；信息密度偏高，符合「运维控制台 + 解题工作台」。
- **排版**：系统 UI 字体栈；数值 / flag / 分数 / 时间戳使用等宽字体（`font-mono`）+ `tabular-nums`。
- **间距**：4px 基准（Tailwind 刻度）；卡片内边距 16/20px，页面容器 `max-w-screen-2xl` + 24/32px 边距。
- **响应式**：`lg` 以上为侧边栏 + 主区（可折叠）；`< lg` 侧边栏收进 `Sheet`，选手端底部为 `BottomNav`，
  表格退化为卡片列表（`DataTable` 的 `renderMobileCard`）。
- **动效**：仅 Radix 自带的 100–200ms 进出场与 `tw-animate-css` 的 `fade-in/slide-in`；无花哨过渡。
- **可访问性**：全部交互可通过键盘完成；焦点环可见（shadcn `ring`）；`Dialog`/`Sheet` 焦点陷阱与
  `aria-*` 交给 Radix；状态色不以颜色为唯一信息载体（始终带文字/图标）；对比度符合 WCAG AA。

## 4. Interaction model

> 视觉风格 ≠ 交互架构。本节显式设计交互，不复刻 Default 的流程；平台只规定能力语义。

- **Primary navigation model**：两个**互相独立**的工作区，各自有整套导航，而不是一套导航下按角色隐藏菜单：
  - **Field（选手端）**：左侧可折叠导航（Play / Community / Me 三组）+ 顶部上下文条 + ⌘K 命令面板。
  - **Control Room（管理端）**：左侧导航 + 顶部「资源搜索」+ ⌘K 命令面板；列表页统一「Sheet 新建/编辑」。
- **Workspace/task model**：一个赛事 = **一个工作区**（`/events/:eventId`），赛事内一切能力以
  **标签面板**在同一工作区内呈现（Overview / Challenges / Arena / Lab / Scoreboard / Trend / Instances / Bulletin）。
  Default 把 AWD 拆成 overview/gameboxes/scoreboard/wireguard/ssh 多页，本前端合并为
  `/arena/:eventId` 驾驶舱：左侧常驻自己的 GameBox + SSH/网络凭据，中部攻击面板（flag 提交 / 目标列表），
  右侧实时积分榜与轮次状态；重置走**行内操作 + 确认对话框**，而非跳页。
  同理 AWDP 的 break/fix/自检/评测合并进 `/lab/:eventId` 单一工作台。
- **Desktop interaction**：三区（导航 / 主区 / 上下文抽屉）；题目详情用「左右分栏」（描述 + 工作面板），
  deep-link 到 `/challenges/:id`；赛事内题目为 `/events/:id/challenges/:challengeId`。
- **Mobile interaction**：底部导航（Play / Events / Community / Me）；表格→卡片；对话框→底部 `Drawer`；
  ⌘K 变成顶栏的搜索按钮（`Sheet` 内命令列表）。
- **Keyboard / command palette**：全局 ⌘K/Ctrl-K；命令来源按当前上下文动态生成
  （跳赛事 / 跳题目 / 提交 flag / 启动实例 / 管理员新建赛事…）；`g e` 跳赛事、`g c` 跳题库等
  简单序列键；`?` 打开快捷键帮助。
- **Modal / drawer / panel strategy**：破坏性操作 = `AlertDialog`；创建 / 编辑 = 右侧 `Sheet`（不打断上下文）；
  长内容阅读 = 路由页面而不是弹窗（可分享、可刷新）；题目 / 实例的快速操作 = 行内 `DropdownMenu`。
- **Destructive action confirmation**：删除赛事 / 题目 / 用户 / 销毁实例 / 重置 GameBox / 结束赛事 /
  删除设置键 / Docker 删除 / SQL 执行 —— 全部 `AlertDialog`，需要输入名字或勾选确认的高危操作（SQL、删除赛事）
  额外要求显式确认文本。**不使用**原生 `alert/confirm`；统一 `toast` 反馈成功/失败。
- **Realtime update presentation**：顶部状态条显示 `SSE 连接 / 重连中 / 已断开` 胶囊（`RealtimePill`），
  事件到达即失效相关 query key；分数变动对变动行做一次高亮脉冲；断线时降级为轮询并显式提示，不静默。
- **空 / 错误 / 权限**：统一 `EmptyState` / `ErrorState`（含重试与错误码）/ `PermissionDenied`；
  任何错误都会渲染可见文案与恢复入口，绝不白屏。

## 5. Information architecture

### 5.1 段落

- **Field（选手端）**：Play（总览 / 赛事 / 题库 / 题集 / 我的实例 / 训练）/ Community（公告 / 讨论 / 解题流水 / 排行 / 题解 / 武器库）/ Me（资料 / 登出）。
- **Control Room（管理端）**：总览 / 赛事 / 内容（题库·题集·GameBox 库）/ 社区（公告·讨论·武器库）/ 平台（用户·超管·日志·设置·前端·计划任务）/ 基础设施（Docker·SQL·终端·版本）。

### 5.2 Route plan（与 Default 无对应关系，自定）

| 路径 | 能力 |
|---|---|
| `/login` `/register` `/forgot` `/reset` | 选手登录 / 注册 / 忘记密码 / 凭 token 重置 |
| `/` | 选手总览（未登录 → 重定向 `/login`） |
| `/events` `/events/:eventId` | 赛事列表 / 赛事工作区 |
| `/events/:eventId/challenges/:challengeId` | 赛事内题目工作台（启动实例 / 提交 flag / 题解） |
| `/challenges` `/challenges/:challengeId` | 练习题库 / 题目工作台 |
| `/challenge-sets` `/challenge-sets/:setId` | 题集列表 / 题集解题 |
| `/instances` | 我的实例（批量销毁） |
| `/arena/:eventId` `/lab/:eventId` | AWD 驾驶舱 / AWDP 工作台（赛事工作区的标签也指向同一组件） |
| `/training` `/training/runs/:runId` | AWDP 练习目录 / 练习 Run 工作台 |
| `/community/announcements` `/community/discussions` `/community/discussions/:id` `/community/discussions/mine` | 公告 / 讨论列表 / 讨论详情 / 我的讨论 |
| `/solves` `/rank` | 解题流水 / Top15 排行 |
| `/writeups` `/writeups/:id` | 题解列表 / 题解详情 |
| `/arsenal` | 武器库 |
| `/profile` | 我的资料（含头像上传） |
| `/admin/login` | 管理端登录 |
| `/admin` | 管理端总览（聚合 + 系统监控 + 版本） |
| `/admin/events` `/admin/events/:eventId` | 赛事管理 / 赛事控制台（config·challenges·members·teams·announcements·instances·logs·writeups·data·network·awd·awdp） |
| `/admin/challenges` `/admin/challenge-sets` `/admin/gameboxes` `/admin/network` | 题库 / 题集 / GameBox 库 / 靶场网络 |
| `/admin/community/announcements` `/admin/community/discussions` `/admin/community/weapons` | 公告 / 讨论 / 武器库治理 |
| `/admin/platform/users` `/admin/platform/super-admins` `/admin/platform/logs` `/admin/platform/settings` `/admin/platform/frontends` `/admin/platform/tasks` | 用户 / 超管 / 日志 / 设置 / 前端 / 计划任务 |
| `/admin/infra/docker` `/admin/infra/sql` `/admin/infra/terminal` `/admin/version` | Docker / SQL 控制台 / Web 终端 / 版本 |
| `*` | 404（可见、可恢复，不是白屏） |

> 路由只服务本前端自己的交互模型；平台不要求 Default 的路径，验收以能力覆盖为准。

## 6. Capability coverage

以 [CAPABILITY-MATRIX.md](../../docs/frontend/CAPABILITY-MATRIX.md) 为基准（`required` 共 89 行）。
逐行状态表在实现完成后落到 `CAPABILITY-COVERAGE.md`（含每行的落地文件与验收证据）。分域计划：

| 域 | required 行数（计划全覆盖） | 落地 |
|---|---|---|
| 平台引导与运行时契约 | 6 | `src/entry.tsx` `src/app/mountApp.tsx`（消费 `mount(context)` 与契约常量；不重复取引导元数据） |
| 认证与会话 | 11 | `src/features/auth/*`、`src/auth/store.ts`、`src/api/client.ts` |
| 选手端：账号与社区内容 | 8 | `src/features/community/*`、`src/features/profile/*` |
| 选手端：赛事与 Jeopardy | 17 | `src/features/events/*`、`src/features/jeopardy/*`、`src/features/sets/*`、`src/features/instances/*`、`src/features/writeups/*` |
| 选手端：AWD | 8 | `src/features/awd/*`（`/arena/:eventId` 驾驶舱 + SSE） |
| 选手端：AWDP 比赛 | 11 | `src/features/awdp/*`（`/lab/:eventId` 工作台 + SSE） |
| 选手端：AWDP Training | 6 | `src/features/training/*`（`/training/runs/:runId` + SSE） |
| 管理端：总览与内容 | 5 | `src/features/admin/core/*` |
| 管理端：赛事管理 | 8 | `src/features/admin/events/*` |
| 管理端：AWD 运维 | 9 | `src/features/admin/events/*` + `src/features/admin/gameboxes/*` |
| 管理端：AWDP 运维 | 6 | `src/features/admin/events/*` |
| 管理端：基础设施、系统与平台设置 | 2 | `src/features/admin/platform/*` |

**结论：complete**（89/89 required）。`optional` 与 `specialized` 行按价值取舍：
optional 尽量实现（题集 / 题解 / 武器库 / writeup / 头像 / 计划任务 / 报告下载 / 装备图片上传 / 系统监控 / 赛事大屏 / 单条查询），
specialized 中 Docker 三页、SQL 控制台、Web 终端、平台网络、前端选择器、赛事 GameBox 实例重置**均实现**，
并在交付说明中声明所用手法（逃生舱）。

## 7. Auth

- **User token**：`zustand` store（`src/auth/store.ts`），持久化到 `localStorage` 的 `shadcn.user.token`；
  读写带异常兜底（隐私模式退化为内存会话）。
- **Admin token**：同一 store 的**独立字段**，键 `shadcn.admin.token`；与选手 token 互不影响。
- **注入**：`createFloatCTFClient({ baseUrl: context.apiBaseUrl, getUserToken, getAdminToken })`；
  SDK 不持有 token、不写存储、不导航。
- **Unauthorized behaviour**：`onUnauthorized({ scope })` → 清对应 scope 的 token +
  该 scope 的 query cache → 跳到 `/login` 或 `/admin/login` 并带 `next`；
  路由守卫（`RequireUser` / `RequireAdmin`）在渲染前判定，避免闪现受保护内容。
- **登出**：清 token + `queryClient.clear()` + 跳登录页。
- **永不**把 token 放进 URL / query string（含 SSE：Bearer 走 `Authorization`，由 SDK 处理）。

## 8. Realtime

| 通道 | 端点（相对 `client.baseUrl` / admin base） | 本前端用法 |
|---|---|---|
| AWD 选手 | `GET /events/<id>/awd/stream` | `useAwdEventStream`（`@floatctf/react`）：轮次 / 分数 / 封禁 / GameBox 状态；事件到达即失效 `awd` 相关 query |
| AWD 管理 | `GET /events/<id>/awd/stream` | `useAdminAwdEventStream`：赛事控制台 AWD 标签 |
| AWDP 选手 | `GET /events/<id>/awdp/stream` | `useAwdpEventStream`：阶段切换 / 评测 / 分数 |
| AWDP 练习 | `GET /service/awdp/runs/<runId>/stream` | `useAwdpRunStream`：练习阶段与评测结果 |

所有实时面板：连接状态胶囊 + 断线降级轮询 + `auth_error` 时停止重连并引导重新登录。

> **逃生舱声明**（AI-FRONTEND-GUIDE §5.4）：管理端 Web 终端用
> `client.adminHttp.post("/terminal/session")` + 原生 `WebSocket`（SDK 无终端抽象，Gap class B）；
> 非 React 路径不需要，因为本前端使用 `@floatctf/react` 的四条流 hook。此外不再使用其它逃生舱；
> 逐项记录见 `CAPABILITY-COVERAGE.md`。

## 9. Artifact

- Entry：`src/entry.tsx` → 构建产物 `assets/frontend.js`（ESM，导出 `mount(context)`）。
- Styles：`src/styles/globals.css` → 构建产物 `assets/frontend.css`（`cssCodeSplit: false`，固定文件名）。
- Build script：`build`（`vite build && tsc --noEmit`）；`build:artifact` 仅构建产物。
- outputDir：`dist/`；构建结束由 Vite 插件写 `dist/frontend.json`（**不含** `build` 段）并用
  `parseFrontendManifest` 真实自检，同时断言入口 / 样式文件存在。
- 源码 manifest：`floatctf.frontend.json`（含 `build` 段，供 `scripts/frontend.sh` 源码安装）。
- 资产：一律通过 `context.assetBaseUrl` 解析（`assetUrl()`），不硬编码站点根。
