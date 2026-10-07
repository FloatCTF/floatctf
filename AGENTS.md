# AGENTS.md — AI 工作手册（Float CTF 平台）

本文件是 AI 编码助手（Pi Coding Agent / Claude Code / Cursor / 其他 agent）在 FloatCTF 仓库内工作的入口。动手前先读相关手册，遵守铁律。

## 仓库一句话

基于 Rust（Actix Web + SeaORM + PostgreSQL + RustFS）与 React 的 CTF 实训/竞赛平台（Jeopardy + AWD 双赛制），monorepo 结构：`apps/api`（后端）、`apps/web`（**Web bootstrap 引导页**）、`frontends/default`（**官方前端 = 当前完整 UI**）、`packages/{sdk,react,frontend-runtime}`（可插拔前端平台）、`crates/`（共享与独立服务）。

## HANDOFF.md（会话交接记忆）

若项目根目录存在 `HANDOFF.md`，说明上一会话/开发者留下了交接记忆：**先完整读取并记住其中的内容（上下文、决策、未完成事项、约束），再开始任何任务**；工作过程中若有新增的关键决策/进展，也及时同步写回该文件，供下次会话延续。该文件是开发者私有的会话记忆，已被 gitignore，不会提交共享。

## 必读文档

| 文档 | 用途 |
|------|------|
| [docs/agents/ARCHITECTURE.md](docs/agents/ARCHITECTURE.md) | 架构速览：模块分层、关键类型、配置体系、数据流 |
| [docs/agents/ADD-FEATURE.md](docs/agents/ADD-FEATURE.md) | **开发新功能**：8 步流程 + 测试清单 |
| [docs/agents/FIX-BUG.md](docs/agents/FIX-BUG.md) | **修 bug**：复现/定位/根因/最小修复/回归 |
| [docs/agents/DATABASE.md](docs/agents/DATABASE.md) | **改数据库**：迁移 → 应用 → 实体/类型再生成（**已有 migrations 不可直接改**） |
| [docs/agents/DATA-FETCHING.md](docs/agents/DATA-FETCHING.md) | **前端数据页面**：缓存分级、keepPreviousData、queryKey 失效 |
| [docs/agents/TESTING.md](docs/agents/TESTING.md) | 测试规范：层级、写法、禁忌 |
| [docs/agents/RULES.md](docs/agents/RULES.md) | **用户反复强调的规则与返工教训**。注意其**作用域**：§1–§4 是 **Default Frontend 专属**（`frontends/default/**`）的形态细则与禁原生弹窗等；§5 环境约定为平台级。新 Frontend 见 [AI-FRONTEND-GUIDE.md](docs/frontend/AI-FRONTEND-GUIDE.md)，**不受** §1–§4 的视觉/交互约束 |
| [docs/frontend/ARCHITECTURE.md](docs/frontend/ARCHITECTURE.md) | **可插拔前端平台**：Frontend ≠ Theme、依赖方向、mount 契约、注册表/版本、信任模型与安全审查 |
| [docs/frontend/DEVELOPING.md](docs/frontend/DEVELOPING.md) | 前端开发：官方前端 / 外部仓库 / 安装激活回滚 |
| [docs/frontend/ARTIFACT.md](docs/frontend/ARTIFACT.md) | 制品：manifest 字段、路径安全规则、注册表 schema、归档格式、构建隔离 |
| [docs/frontend/AI-FRONTEND-GUIDE.md](docs/frontend/AI-FRONTEND-GUIDE.md) | **创建全新 Frontend 的权威手册**（AI 代理作业口径）：Default 只是语义参照、公共依赖边界、mount/auth/路由/SSE、完整性口径、框架配方、验收流程 |
| [docs/frontend/CAPABILITY-MATRIX.md](docs/frontend/CAPABILITY-MATRIX.md) | **前端能力矩阵**（由源码审计生成）：每项能力的公共 SDK 面 / Default 参照 / 是否实时 / 完整前端是否必需 |

## 前端任务先判型（动手前必做）

收到任何「前端」需求，**先判型**。两类任务的目标与约束截然不同，混在一起就是返工。

### A. 修改官方 Default Frontend

- 目标目录：`frontends/default/`（唯一）。
- Default 既有的 Primer、布局、导航、`GenericTable`、`Dialog`、路由结构、视觉约定**就是权威**：保持一致，不要另起炉灶。
- 遵循 [RULES.md](docs/agents/RULES.md) 与同域参照页；优先复用 `components/` 既有组件。
- 触发措辞：「修改官方前端」「改页面」「调整当前 UI」「修改 Default Frontend」。

### B. 创建新的可插拔 Frontend

- 目标：新的 frontend 实现 / **外部仓库**（不是 `frontends/default/`）。
- 先读 [AI-FRONTEND-GUIDE.md](docs/frontend/AI-FRONTEND-GUIDE.md) + [CAPABILITY-MATRIX.md](docs/frontend/CAPABILITY-MATRIX.md)。
- Default **只是语义/行为参照**：业务能力、API 调用方式、鉴权语义、状态语义、错误与边界、实时行为。
  Default **不是**权威的：配色、字体、组件库、布局、页面层级、导航、路由路径、React、Primer、Tailwind、Zustand、TanStack Router/Query —— 这些由新前端**自主决定**。
- **视觉自由 ≠ 只有视觉自由**：**交互模型同样自由** —— 页面转场、工作流、导航层级、任务分组、渐进披露、命令面板、标签式工作区、分屏、抽屉、上下文面板、移动端底部导航、键盘优先 UI、驾驶舱/仪表盘模型都可以重新设计。平台规定的是**能力语义**，不是**点击顺序**：「行为相同」≠「流程相同」。
  （例：Default 走 `Events → AWD Event → GameBoxes → Reset`，新前端走 `Arena → 选中 GameBox → 上下文菜单 → Reset` —— 只要 Reset 的真实语义不变，两者都合法。）
- 只用公共包：`@floatctf/sdk` + `@floatctf/frontend-runtime`（React 才可选 `@floatctf/react`）。
  **禁止** import `frontends/default/*`、`apps/web/*`、`packages/*/src/*`、`@/...`、任何逃逸进 monorepo 的相对路径或私有 alias。
  Default 源码是「示例即文档」，**不是 API**；可以读，不可以依赖。
- **不得**为了创建新前端去修改 `frontends/default`。
- 触发措辞：「新建一个 frontend」「做一个新的前端」「写一个不同风格的 frontend」「创建 FloatCTF frontend」「做一个 cyberpunk/minimal/material frontend」。

未明确限定时，**「创建一个 FloatCTF Frontend」= 完整替换前端**（覆盖 CAPABILITY-MATRIX 中 `required` 的能力），**不是**一个演示落地页。
若用户明确要求 player-only / admin-only / scoreboard-only / kiosk，允许刻意部分实现，但 Agent **必须显式声明自己的覆盖范围**。

### 新 Frontend 硬规则（展开与理由见 [AI-FRONTEND-GUIDE.md](docs/frontend/AI-FRONTEND-GUIDE.md)）

> **总纲**：FloatCTF 规定 Frontend **能做什么**，不规定它**长什么样**、也不规定用户**怎么走到那里**。

1. `Frontend != Theme`：新前端是完整浏览器应用，不是换皮。
2. 除用户明确要求，不要照抄 Default 的 UI。
3. Default = 语义/行为参照，不是视觉模板。
4. 公共边界只有 `@floatctf/sdk` + `@floatctf/frontend-runtime`（React 可选 `@floatctf/react`）。
5. React 绑定是可选的；框架自由（Vue/Svelte/Solid/原生 TS 均可）。
6. 组件库自由：Material / Radix / shadcn / Ant Design / Chakra / Vuetify / 自定义 Web Components / Tailwind / CSS Modules / 原生 CSS / 不用组件库，均可；**不要求使用 Primer**。
7. **`DEFAULT 规则 ≠ 平台规则`**：Default 的 Primer / Octicons / GenericTable / FilterBar / MsgBanner / 弹窗体系 / 现有布局，**只约束 `frontends/default/**`**（见铁律 9、10）。
8. **交互模型自由**：导航层级、任务分组、工作流、页面转场、命令面板、标签式工作区、分屏、抽屉、上下文面板、移动端底部导航、键盘优先 UI 都可以重新设计 —— 只要能力语义 / 权限 / 破坏性操作语义 / 计分与生命周期真相不变（「行为相同」≠「流程相同」）。
9. 浏览器路由归前端所有，平台不要求 Default 的路径；页面数量 / 路由对齐**不是**完整性判据。
10. Token 持久化归前端所有（SDK 不写 `localStorage`、不跳转）；路由守卫与登出 UX 也归前端。
11. 禁止假数据/占位数据充当真实平台数据（与铁律 10 一致）。
12. 「完整替换」= 能力覆盖（见 [CAPABILITY-MATRIX.md](docs/frontend/CAPABILITY-MATRIX.md) 的 `required` 行），不是路由/页面数量对齐。
13. 声明完成前必须通过构建 / 安装 / 端到端验收（见指南的验收流程）。

---

## 铁律（违反即返工）

1. **配置只从 TOML 读取**：`FLOATCTF_CONFIG` → `apps/api/config/development.toml` → `AppConfig`（经 `ReqCtx.config` 注入）。禁止新增环境变量读取；动态设置用 `get_setting`。
2. **Migrations 只前进，无论如何都不可以直接动已有迁移文件**：`apps/api/src/sql/migrations/` 下**已经存在**的文件（含 baseline `20260810121925-initial-schema.sql` / `20260810121926-initial-data.sql`，以及任何已提交或已 apply 的迁移）**绝对禁止**直接修改、删除、重命名、重写、squash、回滚式改写。改 Schema / 修 Schema 错误 / 补数据 **只能** `mise run db:migration:new <名称>` 追加**新**迁移，把 SQL 写进这个新文件。禁止手改 `merged.sql`（生成产物）；禁止在 migration 或业务代码里操作 `schema_migrations`（由 migrate.sh 独占）。唯一允许写入的对象：刚 `new` 出来、尚未 apply、尚未作为历史固化的**新文件**。详细见 [DATABASE.md](docs/agents/DATABASE.md)。
3. **实体是生成的**：手改 `apps/api/src/entity/` / `packages/sdk/src/entity/` 会被覆盖。Schema 变更流程：`db:migration:new` → 写幂等 SQL + 中文 COMMENT（**文件内禁止 BEGIN/COMMIT**）→ `db:migration:validate` → `db:migration:apply` → `mise run db:gen`。`merged.sql` 由 release 流程从 migrations 确定性生成；日常开发启动不依赖它。**sea-orm-cli 必须 1.1.20**。`public.schema_migrations` 不是领域实体（generator 已排除）。API 计算字段（如 settings `resolved_value`）放 manual DTO，不要写回生成文件。
4. **三处一致**：数据库 Schema / 生成实体 / 业务代码引用必须一致（`entity/代码/库` 漂移是历史最高频 bug 源）。
5. **敏感值走 `Secret`**：Debug/日志必须脱敏；`auth.jwt_secret` 等不落日志、不入库。
6. **宿主权限只走 helper**：生产 API 是 Compose 内的非 root 容器，必须保持 `cap_drop=ALL`、`no-new-privileges`、read-only rootfs，禁止挂载 `/var/run/docker.sock`、加入 docker 组或获得 `CAP_NET_ADMIN`；Docker 走 `helper-docker.sock`，WireGuard/nftables/conntrack 走 `helper-control.sock`。新增高权限能力必须先扩展 helper 的受限协议/策略。宿主 systemd 只管理 `floatctf-helper.service` + `floatctf-infra.service`/`floatctf.target`，不要恢复 `floatctf-api.service`。
7. **提交规范**：中文 message（feat/fix/chore/docs/refactor 前缀），按角度分批提交；提交前 `cargo fmt --all && cargo check -p floatctf` 与相关测试全绿。**push 前必须本地完整过一遍验证**：`mise run check` 全绿，前端额外 `tsc --noEmit` 与 `vite build` 通过（CI 跑的是 `vite build && tsc`，本地不绿推送必红）。
8. **先诊断后修复**：修 bug 先定位根因并给证据；涉及行为/数据变更，先向用户说明方案获批后再动手。
9. **修改官方前端时仿照既有页面（仅约束 `frontends/default/`）**：本条的适用范围**只有** `frontends/default/`（即上文「A. 修改官方 Default Frontend」）；创建新的可插拔 Frontend 不受本条约束，见铁律 11 与 [AI-FRONTEND-GUIDE.md](docs/frontend/AI-FRONTEND-GUIDE.md)。在 `frontends/default/` 内改页面时：官方前端位于 `frontends/default/`；新页面先找同域参照页（赛事详情参照 `service/events/jeopardy.$id/*` 与 `awd.$id/*`、管理列表参照 `admin/challenges.tsx`、导航配置参照 `navigation/*`），结构/布局/交互与参照页保持一致；优先复用 `components/` 现有组件（GenericTable、EventStatusBadge、SubmitWriteup、MsgBanner、AppLink、FilterBar 等）与 `@primer/react`，禁止另起炉灶自创视觉风格或手写重复实现。管理页必须用 Challenges 内置 GenericTable 增删改查形态、样式对照要"一模一样"、默认最简方案等细则与全部返工案例见 [RULES.md](docs/agents/RULES.md)。
10. **用户偏好：平台级与 Default 级必须分清（详细见 [RULES.md](docs/agents/RULES.md)）**
    - **平台级 —— 适用所有 Frontend（含新前端）**：展示数据必须来自真实接口，禁止假数据 / 占位数据冒充线上数据（"不要随便搞点数据糊弄我"）；状态判定必须与后端语义一致（生命周期 / 权限 / 计分真相**不得**前端自造）；错误必须可见地处理（不得白屏）；不得泄漏凭据 / token。
    - **Default 级 —— 仅 `frontends/default/**`**：禁止原生 `alert(`/`confirm(`，一律用 Default 既有的 Primer `useConfirm` / `Dialog` / `useMsgBanner`。
    - **新 Frontend 不要求使用 Primer**：它同样应避免原生 `alert/confirm`（出于体验打磨），但**必须用自己的** dialog / modal / toast / banner / 通知体系。
    - 一句话：**`DEFAULT 规则 ≠ 平台规则`** —— 组件库、弹窗方案、视觉与交互形态都是新前端自己的事。
11. **新 Frontend ≠ 改 Default**：创建新的可插拔 Frontend 时，视觉语言、信息架构、导航、路由、布局、组件、设计系统、CSS 策略、状态管理、框架**全部由新前端自主决定**，只共享后端契约与前端运行时契约；判型与 13 条硬规则见上文 [前端任务先判型](#前端任务先判型动手前必做)。**不得**为了创建新前端而修改 `frontends/default`，也不得依赖其源码（只可作行为参照）。发现自己需要改后端才能继续时，先确认是**真实的公共契约缺口**，记录 `PUBLIC SDK GAP: <细节>` 并报给用户，不要擅自改后端。

## 常用命令速查

```bash
mise run setup                              # 新开发机一次性：工具链 + 主机能力 + helper
mise run dev                                # 唯一开发入口：infra + migrations + API watch + Vite
mise run dev:down / dev:reset / dev:logs    # 停止 / 清库重建 / 基础设施日志
mise run db:migration:new <名称>             # 新建 SQL 迁移（文件内无 BEGIN/COMMIT）
mise run db:migration:validate              # 校验迁移文件（不连库）
mise run db:migration:status                # 查看迁移状态（只读）
mise run db:migration:verify                # 校验迁移历史（checksum）
mise run db:migration:apply                 # 执行未应用的迁移，fresh DB 可从 #1 开始
mise run db:migration:merge                 # release/fresh-production 用 merged.sql
mise run db:gen                             # 重新生成 Rust 实体 + TS 类型
mise run fmt / lint / test / check / build   # 质量门禁
mise run web:typecheck                      # 五个前端包 tsc --noEmit
mise run web:architecture                   # 前端架构边界 + 契约版本一致性（失败即红）
mise run dev:packages                       # 改动 packages/* 时以 watch 重建 dist
scripts/frontend.sh list|info|verify|install|remove|set-current   # 前端管理器（生产装在 $FLOATCTF_HOME/frontend.sh）
cargo test -p floatctf <关键词>               # 跑指定单元测试
```

## 开发环境速记

- 唯一开发模式：`mise run dev`；不存在 A/B 双轨与 `floatctf-dev-infra.service`。
- 可插拔前端：`apps/web` 是引导页，官方前端在 `frontends/default`；`mise run dev` 先构建 `packages/*` 再起 Default Frontend 的 Vite（HMR）。前端存储 `$FLOATCTF_HOME/frontends/`，管理器 `$FLOATCTF_HOME/frontend.sh`，激活是设置 `FRONTEND_ACTIVE`，破窗是 `?frontend=default`。
- API：当前开发者 UID 运行，`watchexec` 自动重编译/重启；`dev-api-run.sh` 每次启动时丢弃 docker 等附加组和 capabilities；Web：Vite HMR。
- 宿主控制面：`floatctf-helper.service`，用户 `floatctf-helper` + docker group + `CAP_NET_ADMIN`；API 通过 `/run/floatctf/helper-control.sock` 与 `/run/floatctf/helper-docker.sock` 调用，API 自身无 Docker socket / 网络 capability。
- 开发库：`postgres://postgres:postgres@127.0.0.1:5432/floatctf_db`；fresh DB 直接 `migration apply`，开发启动不需要 `merged.sql`。
- Redis：`redis://127.0.0.1:6379`；RustFS：`http://127.0.0.1:9000`；Caddy/DB/Redis/RustFS 开发端口均回环绑定。Redis 是 API 必需基础设施：正常开发/生产必须提供 `[redis].url` 且 bootstrap PING 成功；禁止重新引入“未配置 Redis 也能正常启动”的生产路径，in-memory/noop 仅限显式测试。
- 配置：`apps/api/config/development.toml`，正常 `awd.network_runtime = "helper"`；`noop` 仅供测试/mock。
- 生产：API/PostgreSQL/Redis/RustFS/Caddy 由生产 Compose 承载（模板内嵌在 `scripts/install.sh`，安装后为 `$FLOATCTF_HOME/compose.prod.yml`，默认根 `/var/lib/floatctf`）；API 不发布宿主 9090，通过 external internal `fctf-platform-control`（API `10.42.8.2`）接收 FlagServer/JudgeServer 回调。生产 TOML 使用 Compose DNS。宿主只创建 `floatctf` 组（不创建该用户），API 容器以数值 `65532:$FLOATCTF_GID` 运行。
- 修改 `crates/floatctf-helper/` / `crates/helper-protocol/` 后重新执行 `mise run setup`，刷新 root-owned `/usr/local/libexec/floatctf-helper`。
