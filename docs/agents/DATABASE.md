# 数据库变更（DATABASE.md）

> 数据库 Schema 变更的完整流程与规范。**任何 Schema 变更 = 迁移 + 应用 + 实体/类型再生成 + 验证**，缺一不可。
> 本文档描述的是 **forward-only migration manager**（`apps/api/src/sql/migrate.sh`）体系：迁移只前进、不回滚；修 Schema 错误 = 再写一个新迁移。

## 三处一致原则（最高优先级）

数据库 Schema、`entity/` 生成实体、业务代码引用，三者必须一致：

```
迁移文件（migrations/*.sql）  ──db:migration:apply──>  实际数据库
      │                                    │
      └───────── db:gen 再生成 ────────────┘
                                               │
业务代码（Column::Xxx / Model.xxx） <── 与实体一致
```

症状对照：
- 实体有、DB 没有 → 启动可能成功，一查询就 `column xxx does not exist`
- 代码引用、实体没有 → 编译错（E0599 `no variant` / E0560 `no field`）
- 实体与 DB 不一致（手改实体 / 用错版本 CLI 生成）→ E0425 等

## 目录与工具

| 路径/命令 | 作用 |
|-----------|------|
| `apps/api/src/sql/migrations/` | 时间戳命名的迁移文件（**唯一 Schema 来源**） |
| `apps/api/src/sql/migrate.sh` | migration manager：new / list / validate / status / verify / apply / make / help |
| `apps/api/src/sql/merged.sql` | 确定性合并产物（`db:migration:merge` 生成，**不受 git 追踪**，fresh DB bootstrap 用） |
| `schema_migrations` 表 | 数据库实际执行记录（version / name / checksum / applied_at），**仅 migrate.sh 管理** |
| `scripts/gen_entities.py` | 从数据库重新生成 Rust 实体（读 FLOATCTF_CONFIG 的 URL） |
| `scripts/gen_web_types.py` | 从 Rust 实体生成前端 TS 类型 |
| mise 任务 | `db:migration:new` / `:list` / `:validate` / `:status` / `:verify` / `:apply` / `:merge`、`db:gen` |

### 数据库三态（apply/status 首先判定）

| 状态 | 判定 | 能否 apply |
|------|------|-----------|
| **EMPTY** | 无任何业务表、无 schema_migrations | ✅ 自动建 schema_migrations 并执行全部迁移 |
| **UNTRACKED** | 有业务表但**无** schema_migrations | ❌ 拒绝（防止对已有 Schema 重放迁移）。需要 baseline 接管（见下） |
| **TRACKED** | 有 schema_migrations | ✅ 只执行 PENDING 的迁移；先校验历史 |

> 当前开发库（floatctf_db）**已于 2026-08-10 完成 Pre-v1 baseline**（migrations/ 已 squash 为 `20260810121925-initial-schema` + `20260810121926-initial-data` 两条，旧 29 条开发历史由 Git 保留），状态 TRACKED，`db:migration:apply` 直接可用。
> 新增一台**全新**机器/CI：DB 为 EMPTY，apply 会从 0 执行全部迁移；或用 merged.sql 一次性初始化（见 merged.sql 节）。
> 遇到 UNTRACKED 库（如接管旧环境）——不要直接动它，向用户说明需要 baseline（生成 schema_migrations + 按迁移最终状态打记录），确认后再做。

## Baseline（Pre-v1 历史 squash）

- **Baseline version**：`20260810121925`（initial-schema）+ `20260810121926`（initial-data）
- **Migration history is frozen from baseline**：`20260810121925-initial-schema.sql` 视为 **IMMUTABLE**，以后任何 Schema 修改**禁止回头改它**，只能 `db:migration:new` 追加新迁移；除非未来明确进行另一次 major-version squash（需单独迁移策略）
- initial-schema：直接描述最终 Schema（Extensions/Types/Casts/Functions/Tables/Constraints/Indexes/Triggers），不含 `schema_migrations`（由 migrate.sh 独占）
- initial-data：仅程序运行必需 bootstrap（内置超级管理员 sysadmin、AWD 网络池默认配置单例）；不含任何 dev/demo/历史数据
- 旧 29 条开发 migration 不再存在（Git 已保留历史）；不要重新建立 old/legacy/archive 目录存放
- 验证基线等价性的方法（如再次做 baseline）：reference DB（旧历史构建）↔ candidate DB（baseline 构建）做 pg_dump 归一化 diff + catalog 语义比较 + bootstrap 数据比较，必须 0 unexplained diff

## 完整流程

### 1. 新建迁移

```bash
mise run db:migration:new add-xxx-column
# 生成 apps/api/src/sql/migrations/YYYYMMDDHHMMSS-add-xxx-column.sql
```

模板（新文件**不含** `BEGIN;`/`COMMIT;`——事务由 migrate.sh 统一管理）：

```sql
-- Migration: 20260810101854-awd-gamebox-single-version
-- 在这里编写迁移 SQL。
```

命名规范：`YYYYMMDDHHMMSS-短英文描述.sql`，描述只含 `a-z0-9-`（如 `add-challenge-solves-event-id`）。timestamp 冲突会自动 +1 秒。

### 2. 编写 SQL（硬性要求）

- **幂等**：`CREATE TABLE IF NOT EXISTS` / `ADD COLUMN IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS`，可重复执行
- **中文注释**：每个新表写 `COMMENT ON TABLE`，每个新列写 `COMMENT ON COLUMN`（注释依据代码语义写清用途，管理员后台/团队协作依赖它）
- 外键带行为：如 `REFERENCES events(id) ON DELETE CASCADE`
- **禁止**在迁移文件里写事务控制：`BEGIN / START TRANSACTION / COMMIT / ROLLBACK` 一律报错（migrate.sh 为每个迁移开独立事务：`BEGIN; <SQL>; INSERT INTO schema_migrations; COMMIT;`，成功才记录、失败整体回滚）
- **禁止**操作 `schema_migrations` 表（validate 会拒绝）
- 数据迁移（UPDATE/INSERT）与 DDL 同文件即可——整体在一个事务里
- 新建 PG 枚举：`CREATE TYPE` 没有 `IF NOT EXISTS`，要包在幂等 `DO $$ ... IF NOT EXISTS (SELECT 1 FROM pg_type ...) THEN CREATE TYPE ...; END IF $$` 里

示例：

```sql
ALTER TABLE "challenge_solves" ADD COLUMN IF NOT EXISTS "event_id" UUID REFERENCES "events"("id") ON DELETE CASCADE;
COMMENT ON COLUMN "challenge_solves"."event_id" IS '所属赛事 ID（NULL=独立/练习解题）';
COMMENT ON TABLE "challenge_solves" IS '独立解题记录：练习模式的解题流水（event_id 为空）；赛事解题另有 event_challenge_solves';
```

> 一个迁移文件做完一件事；多文件按时间戳顺序执行，天然有序。

### 3. 校验（不连数据库）

```bash
mise run db:migration:validate
```

检查：文件名正则、timestamp 唯一、内容非空（仅注释会警告）、无事务控制、无 `schema_migrations` 操作（语句级检测：CREATE/DROP/ALTER/TRUNCATE/INSERT/UPDATE/DELETE）、无非事务安全语句（`CREATE INDEX CONCURRENTLY`/`VACUUM`/`CREATE DATABASE` 等，剥离注释后匹配）。

### 4. 应用到开发库

```bash
mise run db:migration:apply
```

apply 做了什么：
1. validate → 读 `FLOATCTF_CONFIG` 的 `[database].url` → 判定 DB 三态（UNTRACKED 拒绝）
2. 校验已应用历史（checksum / 本地存在 / name 一致）
3. 拿 PostgreSQL advisory lock（`0x464C4154`），锁内基于**数据库当前** schema_migrations 决定每个迁移是否执行（并发 apply 不会重复执行）
4. 逐迁移：`BEGIN; 迁移SQL; INSERT INTO schema_migrations(...); COMMIT;` —— 失败则该迁移整体回滚并终止，已提交的前序保留，修复后重跑 apply 只补失败那条
5. 输出 Applied / Skipped / Pending 汇总

```bash
# 也常配合使用
mise run db:migration:status   # 对比本地 vs 数据库（APPLIED/PENDING/MODIFIED/MISSING LOCAL）
mise run db:migration:verify   # 严格校验已应用历史（本地文件被改过/缺失会报 MODIFIED / MISSING LOCAL）
```

> 旧方式（`docker exec ... psql -f - < 文件` 手动应用）已被 apply 取代。**除非**遇到 UNTRACKED 库且用户同意手动处理，否则一律用 `db:migration:apply`。

### 5. 重新生成实体与类型

```bash
mise run db:gen   # = db:gen:rs（Rust 实体）+ db:gen:ts（Web TS 类型）
```

前置条件：
- 数据库运行中且含最新 Schema（apply 之后）
- **sea-orm-cli 必须为 1.1.20**（`cargo install sea-orm-cli --version 1.1.20 --locked`）
  - 2.0.1 生成的 `rs_type = "Enum"` 与运行时 1.1.20 不兼容 → 全项目编译失败（E0425）
- 生成脚本会**删除并重建** `apps/api/src/entity/`，所以**不要手改实体文件**；`apps/web/src/entity/*.ts` 同样会被覆盖（如 settings.ts 曾有手加字段被冲掉——先 `git stash`/记录再重新生成）

生成后检查：
- `git diff apps/api/src/entity/` 确认新列/新表出现、无关表未被误改
- `cargo check -p floatctf` 确认代码与实体一致

### 6. 验证

```bash
cargo check -p floatctf
cargo test -p floatctf <相关测试>   # 涉及行为的变更跑相关 DB-gated 测试
# 可选：mise run db:migration:verify（确认本地与库一致）
```

## merged.sql（fresh DB bootstrap）

- `mise run db:migration:merge` 从 migrations/ **确定性**生成（同一批文件两次生成 sha256 一致；无时间戳/hostname/路径），chmod 0644
- 内容：`CREATE TABLE IF NOT EXISTS schema_migrations` + 每个迁移 `BEGIN; SQL; INSERT INTO schema_migrations; COMMIT;`（与 apply 同一套 metadata 逻辑）
- **不受 git 追踪**（生成产物）；克隆新仓库后先 `mise run db:migration:merge` 再 `mise run infra:up`
- Docker 容器首次启动时自动执行（`infra/compose/compose.dev.yml` 挂载为 `/docker-entrypoint-initdb.d/00-init.sql`），之后 `db:migration:apply` 接管增量
- 不要手改 merged.sql；永远改 migrations/ 再 merge

## 其他规范

- `.migrate.lock` 是 migrate.sh 的 flock 临时文件，**不入库**（已 gitignore）
- 迁移文件命名：时间戳-简短英文描述（如 `20260807105735-add-challenge-solves-event-id.sql`）
- 删除列/表：优先保留（数据可能还在用）；确需删除时同步检查代码引用与实体（`db:gen` 会移除实体字段）
- 生产环境数据库密码等敏感值不写入迁移文件；迁移只含 Schema 与业务数据，不含凭据
- **已应用迁移的文件内容不可修改**（verify 会报 MODIFIED）；改 Schema 只能新增迁移
- 迁移文件转换约定：文件内不含事务控制（旧模板残留的 `BEGIN;`/`COMMIT;` 由 migrate.sh 校验，重写历史迁移需谨慎——先 `db:migration:verify` 看清影响）
