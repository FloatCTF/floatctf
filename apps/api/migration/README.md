# FloatCTF migrations

Applies **base** (`src/sql/init/`), **AWD** (`src/sql/awd/`), and incremental
updates (`src/sql/update/`) through SeaORM migration history.

SQL files under `src/sql/{init,awd,update}/` remain the **source of truth** —
this crate only orchestrates them via `include_str!` + `execute_unprepared`.
Do not delete those SQL files.

## Stages (R5-D)

| Stage | Status | What |
|-------|--------|------|
| 1 | done | `include_str` SQL + `check` table presence |
| 2 | done | deeper schema check (enums, min column counts) + empty-DB script |
| 3 | done | `baseline` for existing DBs (mark applied, no DDL) |
| 4 | done | Docker starts empty Postgres; app/entrypoint runs migrator |

## Commands

```bash
export DATABASE_URL=postgres://user:pass@localhost:5432/floatctf_db

# from src/floatctf-api (preferred wrappers)
./scripts/migrate.sh up        # apply pending migrations
./scripts/migrate.sh status    # list applied / pending
./scripts/migrate.sh check     # tables + enums + min column counts
./scripts/migrate.sh baseline  # after check OK: record seaql_migrations only

# or from migration crate / workspace-style:
cd migration && cargo run -- up
# equivalent:
cargo run --manifest-path migration/Cargo.toml -- check
```

### Empty-DB verification (stage 2)

```bash
# Skips with exit 0 if DATABASE_URL / RUN_MIGRATION_E2E unset (CI-safe).
./scripts/migration_empty_db_check.sh

# Against a disposable empty database:
DATABASE_URL=postgres://postgres:postgres@localhost:5432/floatctf_db \
  ./scripts/migration_empty_db_check.sh
```

Does **not** require Docker if `DATABASE_URL` already points at an empty Postgres.

### Baseline existing DBs (stage 3)

If tables already exist (legacy Docker `initdb` + manual AWD) but
`seaql_migrations` is empty:

```bash
DATABASE_URL=... ./scripts/baseline_existing_db.sh
# or:
./scripts/migrate.sh baseline
```

Flow: schema `check` → insert pending version rows → future `up` is a no-op
until new migrations are added.

Manual equivalent (if CLI unavailable):

```sql
CREATE TABLE IF NOT EXISTS seaql_migrations (
  version varchar NOT NULL PRIMARY KEY,
  applied_at bigint NOT NULL
);
INSERT INTO seaql_migrations (version, applied_at) VALUES
  ('m0001_base_schema', EXTRACT(EPOCH FROM now())::bigint),
  ('m0002_base_extensions', EXTRACT(EPOCH FROM now())::bigint),
  ('m0100_awd_schema', EXTRACT(EPOCH FROM now())::bigint),
  ('m0101_scheduler_retry', EXTRACT(EPOCH FROM now())::bigint)
ON CONFLICT DO NOTHING;
```

## Migration set

| Version | Source |
|---------|--------|
| `m0001_base_schema` | `src/sql/init/01-up.sql` |
| `m0002_base_extensions` | `02-index.sql`, `03-triggers.sql`, `04-init.sql` |
| `m0100_awd_schema` | `src/sql/awd/01` … `06` |
| `m0101_scheduler_retry` | `src/sql/update/01-scheduler-retry.sql` |

## Docker / devcontainer (stage 4)

Postgres is started **empty** (no app schema mounted into
`docker-entrypoint-initdb.d`). Schema is applied by:

```bash
./scripts/migrate.sh up
# production image entrypoint:
#   /app/docker-entrypoint-migrate.sh /app/floatctf
```

`scripts/docker-entrypoint-migrate.sh` runs `floatctf-migration up` (or the
cargo wrapper) before `exec` of the main process. Set `SKIP_MIGRATE=1` to skip.

**Existing Docker volumes** that were created with the old init mount keep their
data; run `baseline` once so SeaORM history matches, or recreate the volume and
`migrate up` on a fresh DB.

AWD schema is **not** applied manually anymore — it is part of `m0100_awd_schema`.

## Entity regen

After schema changes (and after `migrate up` on a live DB):

```bash
sea-orm-cli generate entity -o src/entity --with-serde both \
  --enum-extra-attributes 'serde(rename_all = "snake_case")'
```

Never hand-edit `src/entity/`.
