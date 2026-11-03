# AGENTS.md — FloatCTF Frontend 仓库

> 把本文件复制到 `my-floatctf-frontend/AGENTS.md`，替换所有 `<...>` 占位符
> （含 `本仓库内联（见下）`，见 §0.1）。
> 本仓库是一个 **FloatCTF Frontend**（可插拔浏览器应用），不是 FloatCTF 平台仓库。

## 0. 这是什么

- 本仓库实现的是一份 **FloatCTF 前端制品**：`FloatCTF Animal Island`
- Frontend id: `animal-island`
- 技术栈: `React 19 + TypeScript + Vite 6 + react-router v7 + TanStack Query + zustand；UI 用 `animal-island-ui` 2.2.1`
- 覆盖范围: `complete（选手端 + 管理端）`（`complete` / `player-only` / `admin-only` / `scoreboard-only` / `kiosk`）
- 视觉方向: `cozy island / kawaii —— 奶油羊皮纸底、暖棕正文、薄荷青主色、2px 描边、大圆角（animal-island-ui 的设计语言）`
- 交互方向: `两个工作区（岛屿 / 控制台）用顶栏切换；卡片网格 + 卡片详情为主，列表页统一「筛选 → 表格/卡片 → 分页」；破坏性操作走统一确认弹窗；⌘/Ctrl+K 快速跳转`（例：workspace/cockpit、keyboard-first、dashboard、
  command palette、mobile-first、content-centric —— 仅为示例，不强制）
- **FloatCTF 兼容目标**：
  - Platform / API contract: `2`（例：`1`）
  - Documentation ref: `本仓库内联（见下）`

### 0.1 文档引用必须钉版本（不要用 `main`）

平台文档随版本演进。本仓库针对**哪个版本**开发，就把文档链接钉在**那个版本**上，
否则一年后 `main` 上是 v1.2/v2 的文档，与你的依赖不再一致：

- 针对**已发布版本**开发：`本仓库内联（见下）` = `v1.0.0`
- 针对**未发布分支**开发：`本仓库内联（见下）` = `release/v1.0`
- 升级平台时：同时升级依赖版本 **与** `本仓库内联（见下）`

权威手册（把 `本仓库内联（见下）` 替换为上面选定的 ref）：

- 在 FloatCTF 仓库内阅读本模板时：[AI-FRONTEND-GUIDE.md](./AI-FRONTEND-GUIDE.md)
- 外部仓库请用（**规范地址**）：
  `https://github.com/FloatCTF/floatctf/blob/本仓库内联（见下）/docs/frontend/AI-FRONTEND-GUIDE.md`
- 能力矩阵：
  `https://github.com/FloatCTF/floatctf/blob/本仓库内联（见下）/docs/frontend/CAPABILITY-MATRIX.md`
- 制品 / manifest：
  `https://github.com/FloatCTF/floatctf/blob/本仓库内联（见下）/docs/frontend/ARTIFACT.md`
- 运行时架构：
  `https://github.com/FloatCTF/floatctf/blob/本仓库内联（见下）/docs/frontend/ARCHITECTURE.md`

**先读根目录的 `FRONTEND-PLAN.md` 再动手**：它记录身份、技术选型、视觉方向、
**交互模型**、信息架构与路由、能力覆盖表、鉴权策略、实时策略与制品配置。任何实现决策必须与它一致；
若要偏离，先更新计划。

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

> **Default 是参考实现，不是 UX 规范。** 它**不是**要你复刻的点击流程。
> 读它是为了搞清**语义与边界情况**，不是为了照抄交互。

## 2. 本仓库自主决定的东西

**平台规定 Frontend「能做什么」；本仓库决定用户「怎么看到、怎么走到」。**
以下全部归本仓库所有，平台不作要求：

- 视觉架构、设计系统、配色、字体、间距、动效、响应式策略
- **交互模型**：导航层级、任务分组、工作流、页面转场、渐进披露、命令面板、
  标签式工作区、分屏、抽屉、上下文面板、移动端底部导航、键盘优先 UI、驾驶舱/仪表盘模型
- 路由与路由路径（不必与 Default 一致）、页面层级、导航结构
- 页面组成：**可以把 Default 的多个页面合并成一个，也可以把一个拆成多个**
- 布局、组件库（Primer 只是 Default 的选择，**本仓库不要求使用它**）、CSS 方案
- 框架与状态库（`React 19 + TypeScript + Vite 6 + react-router v7 + TanStack Query + zustand；UI 用 `animal-island-ui` 2.2.1` 及配套）
- **认证**：token 存哪里、怎么清、登出 UX、路由守卫（SDK 不写 `localStorage`、不跳转）
- **UI**：所有渲染与交互

**Agent 应当主动设计自己的 UX，而不是复现 Default：**

- 同一项能力可以被重组为**完全不同**的工作流，两者都算实现完成。
- **页面数量与路由对齐与本仓库无关**；完整性由能力覆盖判定，不由 URL 判定。
- 交互步骤顺序可以不同：「行为相同」**不等于**「流程相同」。
- Default 源码的唯一用途是理解**语义与边界情况**（含破坏性操作的确认语义、权限边界）。

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
- **语义必须与后端一致**：权限、生命周期（开赛/结束）、计分与判题真相以后端字段为准，
  不得前端自造；破坏性操作（重置环境、删除、下线）的确认语义不得被弱化。
- 每次失败都渲染可见状态；禁止空白屏（loading / empty / permission-denied /
  network error / validation error / expired auth / not-found / realtime 断线都要覆盖）。
- 制品必须满足 `<floatctf-repo>/docs/frontend/ARTIFACT.md`：源码 manifest
  `floatctf.frontend.json` 声明规则；构建产出制品 manifest `frontend.json`
  （不含 `build` 段），并且 `entry` / `styles` 真实存在。
- **版本不可变**：同一 `animal-island` + 同一版本 = 同一份字节。**任何已发布内容变更
  ⇒ 必须升版本号**；同版本改内容会被平台硬拒绝。

## 4. Definition of done

- [ ] `FRONTEND-PLAN.md` 存在（含**交互模型**一节），且实现与之一致
      （含 `complete（选手端 + 管理端）` 声明；partial 已列出缺口）
- [ ] 只依赖 `@floatctf/sdk` + `@floatctf/frontend-runtime`（+ 仅 React 的 `@floatctf/react`）
- [ ] 无任何 FloatCTF monorepo / 私有源码 import
- [ ] `mount(context)` 已实现，使用 `context.apiBaseUrl` 与 `context.assetBaseUrl`
- [ ] 覆盖 `complete（选手端 + 管理端）` 对应的 CAPABILITY-MATRIX 能力；未覆盖项已显式记录
- [ ] 交互模型是**自主设计**的，不是 Default 的页面/导航/点击流程复刻
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
（公共 SDK 未覆盖、但文档明确支持的逃生舱 —— 如 `client.adminHttp` / 原生 `WebSocket` /
同源 `fetch` —— 允许使用，只在交付说明里写清即可。）

---

## 附：animal-island-ui 设计法（本前端的视觉底线）

来源：<https://github.com/guokaigdg/animal-island-ui> 的 `docs/design-system/design-rules.md`
（已对照 v2.2.1 源码核对）。**违反任意一条即返工** —— 这些不是为了好看，
而是因为它们决定了「这看起来是不是同一个应用」。

### 七条设计法
1. 暖棕正文 + 薄荷青主色 + 奶油底；**绝不**用纯黑或冷灰。
2. 圆角最小 12px；按钮与输入框是 50px 胶囊；卡片 20px；Tooltip 16px。
3. 厚 3D 阴影 `0 Npx 0 0 <深色>` **只**用于 `primary` / `danger` 主按钮、Input、Switch；
   `default/dashed/text/link` 只用柔和阴影。
4. 字体 Nunito + Noto Sans SC，字重不低于 400（正文 500，按钮/标题 600–700）。
5. 动效 0.15–0.35s，缓动统一 `cubic-bezier(.4,0,.2,1)`。
6. 焦点环：输入类 `#ffcc00`（黄），按钮/其余 `#19c8b9`（青）；**绝不**用冷蓝。
7. 禁止：直角交互元素、纯黑文字、冷蓝色调、无阴影的扁平设计。

### 十六条硬规则（易犯的几条）
- 文字只用 `#794f27` / `#725d42` / `#8a7b66` / `#9f927d`，不用 `#000` / `#111`。
- **Card 没有 `box-shadow`**；hover 用 `translateY(-2px)`。
- **Switch 外框无阴影**；handle 不用 `box-shadow`。
- **Modal 必须用库自带的 `clip-path: url(#animal-modal-clip)`**，不能换成圆角矩形。
- `Title` 是燕尾缎带（默认 `variant="ribbon"`）；`Card type="title"` 已被移除，标题用 `Title`。
- UI 文本**不用等宽字体**（`CodeBlock` 与终端输出除外）。
- **图标只能来自 `naive-icons`**：不用 emoji、不用 Unicode 符号（✓ ✕ → ←）、
  不手写 `<svg>`、不用其它图标库。
- 不要用 `className` / `style` 去覆盖库组件的颜色/圆角/阴影；自定义元素一律用
  `var(--animal-*)` token 上色，这样才与库同源。

### 常用 token（运行时，`--animal-*` 是公开 API）
`--animal-primary-color #19c8b9`、`--animal-primary-color-bg #e6f9f6`、
`--animal-success-color #6fba2c`、`--animal-warning-color #f5c31c`、
`--animal-error-color #e05a5a`、`--animal-text-color #794f27`、
`--animal-text-color-secondary #9f927d`、`--animal-border-color #dcd8d1`、
`--animal-border-color-light #e8e2d6`、`--animal-bg-color #f8f8f0`、
`--animal-bg-color-secondary #f0e8d8`、`--animal-border-radius-sm/base/lg 16/18/24px`、
`--animal-border-width 2px`、`--animal-spacing-xs…xl 4/8/12/16/24px`、
`--animal-motion-ease cubic-bezier(.4,0,.2,1)`、`--animal-shadow-sm/base/lg`。

**注意**：网上另一份 `css-variables.md` 用的是**短名**（`--animal-primary`、`--animal-bg`、
`--animal-radius`…），那是「不装库、纯手写复刻」的模板。装了 npm 包时**那些名字不存在**，
`var(--animal-bg)` 会静默失效。本仓库只用上面这些**运行时真名**。

### 颜色名（`Card.color` / `Tag.color` / `Badge.color` / `Title.color` 的合法值）
`default` `app-pink` `purple` `app-blue` `app-yellow` `app-orange` `app-teal`
`app-green` `app-red` `lime-green` `yellow-green` `brown` `warm-peach-pink`

### 库没有的东西（必须自己写，且要用 token 自造）
没有 Menu/Dropdown、没有 Layout/Grid、没有 Tree、没有 Steps、没有 Breadcrumb、
没有 Empty、没有 Spin（用 `Loading` 或 `Skeleton`）、没有 Alert。
**没有主题 Provider，也没有暗色模式**（只有 `default.less` 一套浅色）。
