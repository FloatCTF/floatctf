# RULES.md — 用户反复强调的规则与返工教训

> 来源：2026-08 多轮开发中被用户**多次拒绝/纠正后**沉淀的行为准则（前端页面多次返工、GameBox 设计推翻、弹窗/数据要求等）。
> 本文件是 AGENTS.md 铁律的细则与真实案例。**默认照此执行，不要等到返工。**

## 作用域（Scope）—— 先读这一节

**本文件的大部分内容是关于「官方 Default Frontend」的历史返工教训，不是平台契约。**

| 章节 | 作用域 |
|------|--------|
| §1 仿照既有页面、§2 禁止原生弹窗、§4 组件与 API 既有约定 | ✅ **仅 `frontends/default/**`** |
| §3 展示数据必须真实（数据真实性 / 状态语义） | 🌐 **平台级**——所有 Frontend（含新前端）都适用 |
| §5 开发环境与仓库约定 | 🌐 **平台级**（仓库事实，与前端形态无关） |

具体地说，下列内容**只在维护 `frontends/default/**` 时成立**，是保持官方前端一致性的规矩，
**不构成对新可插拔 Frontend 的约束**：

- Primer（`@primer/react`）及 `@primer/octicons-react`
- `GenericTable` / `FilterBar` / `MsgBanner` / `useConfirm` / `Dialog`
- Challenges 页的增删改查形态、`service/events/jeopardy.$id/*` 等同域参照页
- 「样式对照要一模一样」「必须复用 `components/` 既有组件」这类视觉/交互一致性要求

**创建新的可插拔 Frontend 时**：组件库、视觉体系、信息架构、导航、路由、页面组成与**交互模型**
全部由该前端自主决定（可以完全不用 Primer、不用任何组件库）；权威手册是
[docs/frontend/AI-FRONTEND-GUIDE.md](../frontend/AI-FRONTEND-GUIDE.md)，能力清单是
[docs/frontend/CAPABILITY-MATRIX.md](../frontend/CAPABILITY-MATRIX.md)。
判型规则见 [AGENTS.md](../../AGENTS.md) 的「前端任务先判型」。

> 一句话：**`DEFAULT 规则 ≠ 平台规则`**。本文件中凡描述"长什么样 / 怎么点"的条目，作用域都限于 `frontends/default/**`。

## 1. Default Frontend：仿照既有页面（最高频，曾被返工 6+ 次）

> **路径变更（可插拔前端平台）**：当前完整 UI 已迁到 `frontends/default/src/`；
> `apps/web` 现在只是引导页。下面提到的参照页路径请按 `apps/web/src/X` → `frontends/default/src/X` 阅读。

**作用域：仅 `frontends/default/**`。** 核心原则见 AGENTS.md 铁律 9。
以下是用户明确拒绝过的真实案例与具体形态要求——它们是**维护官方前端**的规矩，创建新 Frontend 时不受约束（新前端自主决定页面、导航与交互）。

### 1.1 管理列表/库页：必须用 Challenges 页的内置 GenericTable 形态

- 参照页唯一标准：`frontends/default/src/routes/admin/challenges.tsx`。
- 必须使用 GenericTable **内置**增删改查：`createFn` / `patchFn` / `mutationColumns` / `filterKeys` + `FilterBar`。
- **禁止**：自造普通 HTML 表格（第一版 GameBoxes 库页因此被拒）；自定义 Dialog 表单替代内置 Add/Edit（第二版仍被拒，"还是不太一样啊"）。
- 内置编辑形态需要提交全量配置时，直接用表单编辑整行配置即可；后端对相同内容幂等（digest 去重）不会产生垃圾数据。

### 1.2 选手端赛事页：参照 JeopardyTeam 选手端

- 参照页：`frontends/default/src/routes/service/events/jeopardy.$id/*`。
- 列表用 Primer `Table.Container` + `DataTable`，布局用两栏 flex（真实 flex 比例，如 `flex-[3]`，`flex-28`/`flex-13` 这类 Tailwind 类不存在）。
- 反面案例：AWD 选手端 GameBoxes/Scoreboard 首版被用户评价"太丑了"，按 JeopardyTeam 形态重做后通过。

### 1.3 样式对照是"一模一样"，不是"风格类似"

- 用户原话："直接用文字和 log 那个一模一样即可"、"不需要那个 圆圈"。
- 案例：Event Status 徽章两次返工——先做带圆点的 pill 被拒，最终按 Logs 页 Level 徽章逐项复刻（Primer `Label`、纯文字、无装饰图标/圆点）。
- 交付前**逐项对照**参照页核对（组件、间距、装饰、交互），任何自加装饰都默认视为违规。

### 1.4 已有类似功能的页面/模型：默认最简方案

- 用户原话："我不需要这个设计，我只需要让他和 challenges 一样 单版本即可，我目前还不需要做历史版本规划"。
- 案例：GameBox 曾按"四层模型 + 不可变 Revision N+1 版本历史"实现，被整体推翻为单版本（编辑原地覆盖，同 Challenges）。
- 原则：不要自作主张引入用户未要求的版本历史/多层抽象/多态设计；有异议先说明方案、获批后再做（呼应 AGENTS.md 铁律 6）。

## 2. Default Frontend：禁止原生弹窗

> **作用域：仅 `frontends/default/**`。** 新 Frontend 同样应当避免原生 `alert/confirm`（体验打磨），但**必须使用它自己的** dialog / modal / toast / banner / 通知体系，**不要求**使用 Primer 的 `useConfirm`/`Dialog`/`MsgBanner`。

- 用户明确要求：官方前端代码中**不得出现** `alert(` / `confirm(`（`grep -rn 'alert(\|confirm(' frontends/default/src` 应为 0）。
- 一律使用 `@primer/react` 的 `useConfirm` / `Dialog` / `useMsgBanner` 实现确认、提示与横幅。
- 注意：`MsgBanner` 的 `BannerVariant` 只有 `critical|info|success|upsell|warning`，错误横幅用 `critical`；`useConfirm` 的 `confirmButtonType` 是直接字符串联合类型（`'normal'|'primary'|'danger'`）。

## 3. 平台级：展示数据必须真实（所有 Frontend）

> **作用域：所有 Frontend（含新前端）。** 这条是平台级硬规则，不是 Default 专属；前端可以自由设计展示方式，但**数据来源与状态语义不能自由**。

- 用户原话："不要随便搞点数据糊弄我"、"数据状态一定要准确"。
- 仪表盘、列表、状态徽章等一律来自**真实接口数据**；禁止用假数据/占位值/凭空构造的状态填充页面。
- 状态判定必须与后端数据一致并随数据刷新：赛事 live/upcoming/ended 由 start_time/end_time 计算（无效/缺失日期按 ended/unknown 处理，宁可不显示也不误报进行中）、容器运行与否以真实 status 为准、AWD 状态/阶段以后端字段为准。
- 后端没有对应聚合接口时，先补接口（如 dashboard summary），不要在纯前端拼装/编造。

## 4. Default Frontend：组件与 API 使用的既有约定

> **作用域：仅 `frontends/default/**`。** 新 Frontend 可以自由选择组件库与图标方案（Material / Radix / shadcn / Ant Design / Chakra / Vuetify / 自定义 Web Components / Tailwind / CSS Modules / 原生 CSS / 不用组件库，均可）。

- 复用 `components/` 现有组件（GenericTable、EventStatusBadge、SubmitWriteup、MsgBanner、AppLink、FilterBar 等），不要手写重复实现。
- 图标用 `@primer/octicons-react`（注意个别图标如 CubeIcon/BoxIcon 不存在；Button 用 `leadingVisual` 而非 `leadingIcon`）。
- 前端数据页遵循 `docs/agents/DATA-FETCHING.md`；新页面先读 ADD-FEATURE.md 步骤 5。

## 5. 平台级：开发环境与仓库约定（容易踩坑的事实）

- `mise run dev:api` **是 watchexec 监听进程**（见 `mise.toml`）：改 `apps/api`、`crates/fcmc`、`crates/helper-protocol` 下的 `.rs`/`.toml` 会自动重编译并重启，不需要手动重启。注意它只监听这三个路径，改仓库根或 `scripts/` 不会触发。
- 若曾用历史回退方式手工起过 API（`cargo run`），之后再跑 `mise run dev` 会争抢 9090：先用 `ss -ltnp | grep 9090` 确认只有一个实例，否则旧进程继续提供旧行为、导致验证失效/误判 bug。
- `merged.sql` 是**生成产物，不追踪 git**（由 `mise run db:migration:merge` 重新生成），仅供 release / fresh-production bootstrap 使用；**日常开发不依赖它**（`mise run dev` 会直接对 fresh DB 从 migration #1 应用）。**禁止手改 merged.sql**。
- `chore/` 目录（plans/ 等）被 gitignore，其中的文档是本地工作笔记，不会进入提交。
- **Migrations 绝对禁区**（见 AGENTS.md 铁律 2 与 DATABASE.md）：`apps/api/src/sql/migrations/` 下**已有文件无论如何都不可直接修改/删除/重命名/重写**（含 baseline `initial-schema` / `initial-data`）。改 Schema **只能** `db:migration:new` 追加新迁移；禁止手改生成实体；禁止操作 `schema_migrations` 表。
