-- Historical / ad-hoc incremental updates (pre-update/ directory).
-- Prefer new files under src/sql/update/ (numbered, idempotent).
--
-- Already-applied historical patches:
ALTER TABLE users ADD COLUMN IF NOT EXISTS avatar TEXT DEFAULT NULL;

-- After this file, apply ordered scripts in src/sql/update/ when not using
-- floatctf-migration, for example:
--   psql "$DATABASE_URL" -f src/sql/update/01-scheduler-retry.sql
