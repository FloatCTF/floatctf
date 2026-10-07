# AGENTS.md — FloatCTF Frontend 仓库

> 把本文件复制到 `my-floatctf-frontend/AGENTS.md`，替换所有 `<...>` 占位符。
> 本仓库是一个 **FloatCTF Frontend**（可插拔浏览器应用），不是 FloatCTF 平台仓库。

## 0. 这是什么

- 本仓库实现的是一份 **FloatCTF 前端制品**：`<FRONTEND_NAME>`
- Frontend id: `<FRONTEND_ID>`
- 技术栈: `<FRAMEWORK>`
- 覆盖范围: `<SCOPE>`（`complete` / `player-only` / `admin-only` / `scoreboard-only` / `kiosk`）
- 视觉方向: `<VISUAL_DIRECTION>`

**先读 `<...>/FRONTEND-PLAN.md`（本仓库根目录）再动手**：它记录身份、技术选型、视觉方向、
信息架构与路由、能力覆盖表、鉴权策略、实时策略与制品配置。任何实现决策必须与它一致；
若要偏离，先更新计划。

权威手册（FloatCTF 平台侧，先读）：
- 在 FloatCTF 仓库内阅读本模板时：[AI-FRONTEND-GUIDE.md](./AI-FRONTEND-GUIDE.md)
- 外部仓库（AGENTS.md 已被复制出去）请用下面的 GitHub 地址
- 规范地址（外部仓库请用这个）：
  https://github.com/FloatCTF/floatctf/blob/main/docs/frontend/AI-FRONTEND-GUIDE.md
- 能力矩阵：https://github.com/FloatCTF/floatctf/blob/main/docs/frontend/CAPABILITY-MATRIX.md
- 制品 / manifest：https://github.com/FloatCTF/floatctf/blob/main/docs/frontend/ARTIFACT.md
- 运行时架构：https://github.com/FloatCTF/floatctf/blob/main/docs/frontend/ARCHITECTURE.md

## 1. 依赖边界（硬规则）

**只用公共包**：

- `@floatctf/sdk`（必需）：传输、领域 API、DTO、SSE、错误模型
- `@floatctf/frontend-runtime`（必需）：`mount(context)` 契约、manifest 校验器、契约常量
- `@floatctf/react`（可选，**仅** React）：headless hooks，无 UI

**MUST NOT** import FloatCTF monorepo / 私有源码：

- ❌ `apps/web/*`、`frontends/default/*`、`frontends/default/src/*`
- ❌ `packages/*/src/*`（例如 `@floatctf/sdk/src/...`）
- ❌ `@/...` 或任何 FloatCTF 私有 alias
- ❌ 任何逃逸进 FloatCTF 仓库的相对路径

`frontends/default`（官方 Default Frontend）**只是行为/语义参照**：可以阅读它来理解业务能力、
API 调用方式、鉴权语义、状态语义、错误与边界、实时行为，以及用户/管理员有权看到什么。
它**不是**视觉模板，也**永远不是**依赖。

## 2. 本仓库自主决定的东西

以下全部归本仓库所有，平台不作要求：

- 视觉架构、设计系统、配色、字体、间距、动效、响应式策略
- 路由与路由路径（不必与 Default 一致）、页面层级、导航结构
- 布局、组件库、CSS 方案
- 框架与状态库（`<FRAMEWORK>` 及配套）
- **认证**：token 存哪里、怎么清、登出 UX、路由守卫（SDK 不写 `localStorage`、不跳转）
- **UI**：所有渲染与交互

## 3. 必须遵守的平台契约

- 制品入口导出 `mount(context)`，实现
  `FloatCTFFrontendModule`（见 `@floatctf/frontend-runtime`）。
- **MUST** 用 `context.apiBaseUrl` 建 SDK 客户端（生产为同源，默认 `/api`）。
- **MUST** 用 `context.assetBaseUrl` 加载本前端自己的资产（生产路径**带版本号**，
  不要假设前端位于站点根 `/`）。
- 保留 `context.root` 作为唯一渲染宿主；返回清理函数以支持卸载。
- token **绝不**放进 URL / query string；Bearer 只走 `Authorization` 头。
- **真实 API 数据 only**：禁止假数据 / mock / 占位数据充当 live 平台数据。
  mock 只允许出现在隔离的测试 / fixture 中，绝不随制品发布。
- 每次失败都渲染可见状态；禁止空白屏（loading / empty / permission-denied /
  network error / validation error / expired auth / not-found / realtime 断线都要覆盖）。
- 制品必须满足 `<floatctf-repo>/docs/frontend/ARTIFACT.md`：源码 manifest
  `floatctf.frontend.json` 声明规则；构建产出制品 manifest `frontend.json`
  （不含 `build` 段），并且 `entry` / `styles` 真实存在。
- **版本不可变**：同一 `<FRONTEND_ID>` + 同一版本 = 同一份字节。**任何已发布内容变更
  ⇒ 必须升版本号**；同版本改内容会被平台硬拒绝。

## 4. Definition of done

- [ ] `FRONTEND-PLAN.md` 存在，且实现与之一致（含 `<SCOPE>` 声明；partial 已列出缺口）
- [ ] 只依赖 `@floatctf/sdk` + `@floatctf/frontend-runtime`（+ 仅 React 的 `@floatctf/react`）
- [ ] 无任何 FloatCTF monorepo / 私有源码 import
- [ ] `mount(context)` 已实现，使用 `context.apiBaseUrl` 与 `context.assetBaseUrl`
- [ ] 覆盖 `<SCOPE>` 对应的 CAPABILITY-MATRIX 能力；未覆盖项已显式记录
- [ ] loading / empty / error / unauthorized / not-found / realtime 断线状态都有 UI
- [ ] 全部展示数据来自真实 API（无假数据）
- [ ] token 存储、登出、路由守卫由本仓库实现；token 从不出现在 URL
- [ ] 生产构建成功，且 `dist/` 内 `frontend.json` 通过 manifest 校验
- [ ] 测试通过（`pnpm test` / `npm test` 或等价命令）
- [ ] 深链直接刷新可用（SPA fallback）
- [ ] 版本号已按内容变更递增
- [ ] 在 FloatCTF 实例上完成安装与验收（`frontend.sh install` → 激活 → 刷新 →
      真实数据 → `?frontend=default` 回退）

## 5. 交付前

跑构建与测试，**不得**仅以「构建成功」宣布完成。若发现必须修改 FloatCTF 后端/公共包
才能继续，停下来，输出 `PUBLIC SDK GAP: <细节>` 并上报，**不要**擅自修改上游仓库。
