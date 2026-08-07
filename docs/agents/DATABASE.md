# 数据库变更（DATABASE.md）

> 数据库 Schema 变更的完整流程与规范。**任何 Schema 变更 = 迁移 + 应用 + 实体/类型再生成 + 验证**，缺一不可。

## 三处一致原则（最高优先级）

数据库 Schema、`entity/` 生成实体、业务代码引用，三者必须一致：

```
迁移文件（migrations/*.sql）  ──应用到──>  实际数据库
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
| `apps/api/src/sql/migrations/` | 时间戳命名的迁移文件（唯一 Schema 来源） |
| `apps/api/src/sql/merged.sql` | 按顺序合并的全部迁移（`migrate.sh make` 生成） |
| `apps/api/src/sql/migrate.sh` | `new <名称>` 新建、`make` 合并、`list` 列出 |
| `scripts/gen_entities.py` | 从数据库重新生成 Rust 实体（读 FLOATCTF_CONFIG 的 URL） |
| `scripts/gen_web_types.py` | 从 Rust 实体生成前端 TS 类型 |
| mise 任务 | `db:migration:new`、`db:migration:merge`、`db:gen` |

## 完整流程

### 1. 新建迁移

```bash
mise run db:migration:new add-xxx-column
# 生成 apps/api/src/sql/migrations/YYYYMMDDHHMMSS-add-xxx-column.sql（BEGIN/COMMIT 模板）
```

### 2. 编写 SQL

要求（硬性）：
- **幂等**：`CREATE TABLE IF NOT EXISTS` / `ADD COLUMN IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS`，可重复执行
- **中文注释**：每个新表写 `COMMENT ON TABLE`，每个新列写 `COMMENT ON COLUMN`（注释依据代码语义写清用途，管理员后台/团队协作依赖它）
- 外键带行为：如 `REFERENCES events(id) ON DELETE CASCADE`
- 数据迁移（UPDATE）单独成文件或文件内事务内完成

示例：

```sql
BEGIN;
ALTER TABLE "challenge_solves" ADD COLUMN IF NOT EXISTS "event_id" UUID REFERENCES "events"("id") ON DELETE CASCADE;
COMMENT ON COLUMN "challenge_solves"."event_id" IS '所属赛事 ID（NULL=独立/练习解题）';
COMMENT ON TABLE "challenge_solves" IS '独立解题记录：练习模式的解题流水（event_id 为空）；赛事解题另有 event_challenge_solves';
COMMIT;
```

### 3. 合并并应用到开发库

```bash
mise run db:migration:merge   # 重新生成 merged.sql

# 应用单个新迁移（开发库：postgres/postgres/floatctf_db）
docker exec -i floatctf-dev-db psql -U postgres -d floatctf_db -v ON_ERROR_STOP=1 \
  -f - < apps/api/src/sql/migrations/XXXXXX-xxx.sql
```

> 注意：
> - `docker exec ... -f 文件名` 读的是**容器内**路径；宿主文件必须用 stdin（`-f - < 文件`）
> - 开发库没有迁移跟踪表，手动应用；生产部署应另行规划（合并后的 merged.sql 一次性执行）
> - 迁移文件为幂等 SQL，重复应用安全

### 4. 重新生成实体与类型

```bash
mise run db:gen   # = db:gen:rs（Rust 实体）+ db:gen:ts（Web TS 类型）
```

前置条件：
- 数据库运行中且含最新 Schema
- **sea-orm-cli 必须为 1.1.20**（`cargo install sea-orm-cli --version 1.1.20 --locked`）
  - 2.0.1 生成的 `rs_type = "Enum"` 与运行时 1.1.20 不兼容 → 全项目编译失败（E0425）
- 生成脚本会**删除并重建** `apps/api/src/entity/`，所以**不要手改实体文件**

生成后检查：
- `git diff apps/api/src/entity/` 确认新列/新表出现、无关表未被误改
- 库里不存在的表不会生成实体（如 kv_store/oob_records/oob_tokens 已随删除消失）
- `cargo check -p floatctf` 确认代码与实体一致

### 5. 验证

```bash
cargo check -p floatctf
cargo test -p floatctf core::config    # 配置相关（如有）
# 可选：docker exec floatctf-dev-db psql -U postgres -d floatctf_db -c '\d 表名'
```

## 其他规范

- `.migrate.lock` 是 migrate.sh 的 flock 临时文件，**不入库**（已 gitignore）
- 迁移文件命名：时间戳-简短英文描述（如 `20260807105735-add-challenge-solves-event-id.sql`）
- 删除列/表：优先保留（数据可能还在用）；确需删除时同步检查代码引用与实体（`db:gen` 会移除实体字段）
- 生产环境数据库密码等敏感值不写入迁移文件；迁移只含 Schema 与业务数据，不含凭据
