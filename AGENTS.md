# AGENTS.md — AI 工作手册（Float CTF 平台）

本文件是 AI 编码助手（Pi Coding Agent / Claude Code / Cursor / 其他 agent）在 FloatCTF 仓库内工作的入口。动手前先读相关手册，遵守铁律。

## 仓库一句话

基于 Rust（Actix Web + SeaORM + PostgreSQL + RustFS）与 React 的 CTF 实训/竞赛平台（Jeopardy + AWD 双赛制），monorepo 结构：`apps/api`（后端）、`apps/web`（前端）、`crates/`（共享与独立服务）。

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
| [docs/agents/RULES.md](docs/agents/RULES.md) | **用户反复强调的规则与返工教训**：前端形态细则、禁原生弹窗、数据必须真实、环境约定 |

## 铁律（违反即返工）

1. **配置只从 TOML 读取**：`FLOATCTF_CONFIG` → `apps/api/config/development.toml` → `AppConfig`（经 `ReqCtx.config` 注入）。禁止新增环境变量读取；动态设置用 `get_setting`。
2. **Migrations 只前进，无论如何都不可以直接动已有迁移文件**：`apps/api/src/sql/migrations/` 下**已经存在**的文件（含 baseline `20260810121925-initial-schema.sql` / `20260810121926-initial-data.sql`，以及任何已提交或已 apply 的迁移）**绝对禁止**直接修改、删除、重命名、重写、squash、回滚式改写。改 Schema / 修 Schema 错误 / 补数据 **只能** `mise run db:migration:new <名称>` 追加**新**迁移，把 SQL 写进这个新文件。禁止手改 `merged.sql`（生成产物）；禁止在 migration 或业务代码里操作 `schema_migrations`（由 migrate.sh 独占）。唯一允许写入的对象：刚 `new` 出来、尚未 apply、尚未作为历史固化的**新文件**。详细见 [DATABASE.md](docs/agents/DATABASE.md)。
3. **实体是生成的**：手改 `apps/api/src/entity/` / `apps/web/src/entity/` 会被覆盖。Schema 变更流程：`db:migration:new` → 写幂等 SQL + 中文 COMMENT（**文件内禁止 BEGIN/COMMIT**）→ `db:migration:validate` → `db:migration:apply` → `mise run db:gen` → `db:migration:merge`。**sea-orm-cli 必须 1.1.20**。`public.schema_migrations` 不是领域实体（generator 已排除）。API 计算字段（如 settings `resolved_value`）放 manual DTO，不要写回生成文件。
4. **三处一致**：数据库 Schema / 生成实体 / 业务代码引用必须一致（`entity/代码/库` 漂移是历史最高频 bug 源）。
5. **敏感值走 `Secret`**：Debug/日志必须脱敏；`auth.jwt_secret` 等不落日志、不入库。
6. **提交规范**：中文 message（feat/fix/chore/docs/refactor 前缀），按角度分批提交；提交前 `cargo fmt --all && cargo check -p floatctf` 与相关测试全绿。**push 前必须本地完整过一遍验证**：`mise run check` 全绿，前端额外 `tsc --noEmit` 与 `vite build` 通过（CI 跑的是 `vite build && tsc`，本地不绿推送必红）。
7. **先诊断后修复**：修 bug 先定位根因并给证据；涉及行为/数据变更，先向用户说明方案获批后再动手。
8. **前端仿照既有页面**：新页面先找同域参照页（赛事详情参照 `service/events/jeopardy.$id/*` 与 `awd.$id/*`、管理列表参照 `admin/challenges.tsx`、导航配置参照 `navigation/*`），结构/布局/交互与参照页保持一致；优先复用 `components/` 现有组件（GenericTable、EventStatusBadge、SubmitWriteup、MsgBanner、AppLink、FilterBar 等）与 `@primer/react`，禁止另起炉灶自创视觉风格或手写重复实现。管理页必须用 Challenges 内置 GenericTable 增删改查形态、样式对照要"一模一样"、默认最简方案等细则与全部返工案例见 [RULES.md](docs/agents/RULES.md)。
9. **用户偏好（详细见 [RULES.md](docs/agents/RULES.md)）**：禁止原生 `alert(`/`confirm(` 弹窗（一律用 Primer `useConfirm`/`Dialog`/`useMsgBanner`）；展示数据必须来自真实接口、禁止假数据/占位糊弄（"不要随便搞点数据糊弄我"），状态判定必须与后端一致。

## 常用命令速查

```bash
mise run infra:up / infra:logs / infra:down   # 基础设施（Postgres/RustFS/Nginx）
mise run dev:api / dev:web / dev              # 启动开发服务
mise run db:migration:new <名称>               # 新建 SQL 迁移（文件内无 BEGIN/COMMIT）
mise run db:migration:validate                # 校验迁移文件（不连库）
mise run db:migration:status                  # 查看迁移状态（只读）
mise run db:migration:verify                  # 校验迁移历史（checksum）
mise run db:migration:apply                   # 执行未应用的迁移（开发库已 baseline）
mise run db:migration:merge                   # 合并迁移 → merged.sql（fresh DB bootstrap；生成产物不提交 git）
mise run db:gen                               # 重新生成 Rust 实体 + TS 类型
mise run fmt / lint / test / check / build     # 质量门禁
cargo test -p floatctf <关键词>                 # 跑指定单元测试
```

## 开发环境速记

- API：`http://localhost:9090`（mise run dev:api）；Web：`http://localhost:3000`；统一入口 `http://localhost:7780`（Nginx）
- 开发库：`postgres://postgres:postgres@127.0.0.1:5432/floatctf_db`（容器 floatctf-dev-db）
- 对象存储：RustFS `http://127.0.0.1:9000`（桶 `floatctf-public` / `floatctf-private`）
- 配置样例：`apps/api/config/development.toml`；启动日志：`WORK_DIR/logs/api/`（按天滚动，开发库 WORK_DIR=`../../app` → `app/logs/api/`）
- **`mise run dev:api` 不是 watch 进程**：改后端代码后必须手动重启（kill 9090 端口进程 → `cd apps/api && setsid nohup cargo run > /tmp/dev-api.log 2>&1 & disown`），否则旧进程继续提供旧行为（曾导致验证失效/误判 bug）
