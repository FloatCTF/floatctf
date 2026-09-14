# RULES.md — 用户反复强调的规则与返工教训

> 来源：2026-08 多轮开发中被用户**多次拒绝/纠正后**沉淀的行为准则（前端页面多次返工、GameBox 设计推翻、弹窗/数据要求等）。
> 本文件是 AGENTS.md 铁律的细则与真实案例。**默认照此执行，不要等到返工。**

## 1. 前端仿照既有页面（最高频，曾被返工 6+ 次）

核心原则见 AGENTS.md 铁律 7。以下是用户明确拒绝过的真实案例与具体形态要求：

### 1.1 管理列表/库页：必须用 Challenges 页的内置 GenericTable 形态

- 参照页唯一标准：`apps/web/src/routes/admin/challenges.tsx`。
- 必须使用 GenericTable **内置**增删改查：`createFn` / `patchFn` / `mutationColumns` / `filterKeys` + `FilterBar`。
- **禁止**：自造普通 HTML 表格（第一版 GameBoxes 库页因此被拒）；自定义 Dialog 表单替代内置 Add/Edit（第二版仍被拒，"还是不太一样啊"）。
- 内置编辑形态需要提交全量配置时，直接用表单编辑整行配置即可；后端对相同内容幂等（digest 去重）不会产生垃圾数据。

### 1.2 选手端赛事页：参照 JeopardyTeam 选手端

- 参照页：`apps/web/src/routes/service/events/jeopardy.$id/*`。
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

## 2. 禁止原生弹窗

- 用户明确要求：前端代码中**不得出现** `alert(` / `confirm(`（全仓 grep 应为 0）。
- 一律使用 `@primer/react` 的 `useConfirm` / `Dialog` / `useMsgBanner` 实现确认、提示与横幅。
- 注意：`MsgBanner` 的 `BannerVariant` 只有 `critical|info|success|upsell|warning`，错误横幅用 `critical`；`useConfirm` 的 `confirmButtonType` 是直接字符串联合类型（`'normal'|'primary'|'danger'`）。

## 3. 展示数据必须真实

- 用户原话："不要随便搞点数据糊弄我"、"数据状态一定要准确"。
- 仪表盘、列表、状态徽章等一律来自**真实接口数据**；禁止用假数据/占位值/凭空构造的状态填充页面。
- 状态判定必须与后端数据一致并随数据刷新：赛事 live/upcoming/ended 由 start_time/end_time 计算（无效/缺失日期按 ended/unknown 处理，宁可不显示也不误报进行中）、容器运行与否以真实 status 为准、AWD 状态/阶段以后端字段为准。
- 后端没有对应聚合接口时，先补接口（如 dashboard summary），不要在纯前端拼装/编造。

## 4. 前端组件与 API 使用的既有约定

- 复用 `components/` 现有组件（GenericTable、EventStatusBadge、SubmitWriteup、MsgBanner、AppLink、FilterBar 等），不要手写重复实现。
- 图标用 `@primer/octicons-react`（注意个别图标如 CubeIcon/BoxIcon 不存在；Button 用 `leadingVisual` 而非 `leadingIcon`）。
- 前端数据页遵循 `docs/agents/DATA-FETCHING.md`；新页面先读 ADD-FEATURE.md 步骤 5。

## 5. 开发环境与仓库约定（容易踩坑的事实）

- `mise run dev:api` **不是 watch 进程**：改后端代码后必须手动重启（kill 9090 端口进程 → `cd apps/api && setsid nohup cargo run > /tmp/dev-api.log 2>&1 & disown`），否则旧进程继续提供旧行为，导致验证失效/误判 bug。
- `merged.sql` 是**生成产物，不追踪 git**（由 `mise run db:migration:merge` 重新生成；fresh clone 需先运行 merge 再 infra:up）。**禁止手改 merged.sql**。
- `chore/` 目录（plans/ 等）被 gitignore，其中的文档是本地工作笔记，不会进入提交。
- **Migrations 绝对禁区**（见 AGENTS.md 铁律 2 与 DATABASE.md）：`apps/api/src/sql/migrations/` 下**已有文件无论如何都不可直接修改/删除/重命名/重写**（含 baseline `initial-schema` / `initial-data`）。改 Schema **只能** `db:migration:new` 追加新迁移；禁止手改生成实体；禁止操作 `schema_migrations` 表。
