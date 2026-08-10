# AGENTS.md — AI 工作手册（Float CTF 平台）

本文件是 AI 编码助手（Pi Coding Agent / Claude Code / Cursor / 其他 agent）在 FloatCTF 仓库内工作的入口。动手前先读相关手册，遵守铁律。

## 仓库一句话

基于 Rust（Actix Web + SeaORM + PostgreSQL + RustFS）与 React 的 CTF 实训/竞赛平台（Jeopardy + AWD 双赛制），monorepo 结构：`apps/api`（后端）、`apps/web`（前端）、`crates/`（共享与独立服务）。

## 必读文档

| 文档 | 用途 |
|------|------|
| [docs/agents/ARCHITECTURE.md](docs/agents/ARCHITECTURE.md) | 架构速览：模块分层、关键类型、配置体系、数据流 |
| [docs/agents/ADD-FEATURE.md](docs/agents/ADD-FEATURE.md) | **开发新功能**：8 步流程 + 测试清单 |
| [docs/agents/FIX-BUG.md](docs/agents/FIX-BUG.md) | **修 bug**：复现/定位/根因/最小修复/回归 |
| [docs/agents/DATABASE.md](docs/agents/DATABASE.md) | **改数据库**：迁移 → 应用 → 实体/类型再生成 |
| [docs/agents/DATA-FETCHING.md](docs/agents/DATA-FETCHING.md) | **前端数据页面**：缓存分级、keepPreviousData、queryKey 失效 |
| [docs/agents/TESTING.md](docs/agents/TESTING.md) | 测试规范：层级、写法、禁忌 |

## 铁律（违反即返工）

1. **配置只从 TOML 读取**：`FLOATCTF_CONFIG` → `apps/api/config/development.toml` → `AppConfig`（经 `ReqCtx.config` 注入）。禁止新增环境变量读取；动态设置用 `get_setting`。
2. **实体是生成的**：手改 `apps/api/src/entity/` 会被覆盖。Schema 变更必须：`mise run db:migration:new` → 写幂等 SQL + 中文 COMMENT（**文件内禁止 BEGIN/COMMIT，事务由 migrate.sh 管理**）→ `db:migration:validate` → `db:migration:apply`（开发库已 baseline，直接可用）→ `mise run db:gen`。**sea-orm-cli 必须 1.1.20**（2.0.1 生成产物会导致全项目编译失败）。详细流程见 [DATABASE.md](docs/agents/DATABASE.md)。
3. **三处一致**：数据库 Schema / 生成实体 / 业务代码引用必须一致（`entity/代码/库` 漂移是历史最高频 bug 源）。
4. **敏感值走 `Secret`**：Debug/日志必须脱敏；`auth.jwt_secret` 等不落日志、不入库。
5. **提交规范**：中文 message（feat/fix/chore/docs/refactor 前缀），按角度分批提交；提交前 `cargo fmt --all && cargo check -p floatctf` 与相关测试全绿。**push 前必须本地完整过一遍验证**：`mise run check` 全绿，前端额外 `tsc --noEmit` 与 `vite build` 通过（CI 跑的是 `vite build && tsc`，本地不绿推送必红）。
6. **先诊断后修复**：修 bug 先定位根因并给证据；涉及行为/数据变更，先向用户说明方案获批后再动手。
7. **前端仿照既有页面**：新页面先找同域参照页（赛事详情参照 `service/events/jeopardy.$id/*` 与 `awd.$id/*`、管理列表参照 `admin/challenges.tsx`、导航配置参照 `navigation/*`），结构/布局/交互与参照页保持一致；优先复用 `components/` 现有组件（GenericTable、EventStatusBadge、SubmitWriteup、MsgBanner、AppLink、FilterBar 等）与 `@primer/react`，禁止另起炉灶自创视觉风格或手写重复实现。

## 常用命令速查

```bash
mise run infra:up / infra:logs / infra:down   # 基础设施（Postgres/RustFS/Nginx）
mise run dev:api / dev:web / dev              # 启动开发服务
mise run db:migration:new <名称>               # 新建 SQL 迁移（文件内无 BEGIN/COMMIT）
mise run db:migration:validate                # 校验迁移文件（不连库）
mise run db:migration:status                  # 查看迁移状态（只读）
mise run db:migration:verify                  # 校验迁移历史（checksum）
mise run db:migration:apply                   # 执行未应用的迁移（开发库已 baseline）
mise run db:migration:merge                   # 合并迁移 → merged.sql（fresh DB bootstrap）
mise run db:gen                               # 重新生成 Rust 实体 + TS 类型
mise run fmt / lint / test / check / build     # 质量门禁
cargo test -p floatctf <关键词>                 # 跑指定单元测试
```

## 开发环境速记

- API：`http://localhost:9090`（mise run dev:api）；Web：`http://localhost:3000`；统一入口 `http://localhost:7780`（Nginx）
- 开发库：`postgres://postgres:postgres@127.0.0.1:5432/floatctf_db`（容器 floatctf-dev-db）
- 对象存储：RustFS `http://127.0.0.1:9000`（桶 `floatctf-public` / `floatctf-private`）
- 配置样例：`apps/api/config/development.toml`；启动日志：`WORK_DIR/logs/api/`（按天滚动，开发库 WORK_DIR=`../../app` → `app/logs/api/`）
