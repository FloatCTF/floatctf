#!/usr/bin/env bash
# Stage 2: empty-DB schema verification.
#
# Runs `migrate up` then `check` against DATABASE_URL.
# Skips (exit 0) when neither DATABASE_URL nor RUN_MIGRATION_E2E=1 is set —
# safe for CI jobs that do not provision Postgres.
#
# Usage:
#   DATABASE_URL=postgres://... ./scripts/migration_empty_db_check.sh
#   RUN_MIGRATION_E2E=1 DATABASE_URL=postgres://... ./scripts/migration_empty_db_check.sh
#
# Does not start Docker; point DATABASE_URL at an empty (or disposable) DB.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if [[ -z "${DATABASE_URL:-}" ]]; then
  if [[ -f "$ROOT/.env" ]]; then
    # shellcheck disable=SC1091
    set -a
    source "$ROOT/.env"
    set +a
  fi
fi

if [[ -z "${DATABASE_URL:-}" && "${RUN_MIGRATION_E2E:-}" != "1" ]]; then
  echo "migration_empty_db_check: skip (set DATABASE_URL or RUN_MIGRATION_E2E=1)"
  exit 0
fi

if [[ -z "${DATABASE_URL:-}" ]]; then
  echo "migration_empty_db_check: RUN_MIGRATION_E2E=1 requires DATABASE_URL" >&2
  exit 1
fi

export DATABASE_URL

echo "migration_empty_db_check: migrate up → $DATABASE_URL"
"$ROOT/scripts/migrate.sh" up

echo "migration_empty_db_check: schema check"
"$ROOT/scripts/migrate.sh" check

echo "migration_empty_db_check: OK"
