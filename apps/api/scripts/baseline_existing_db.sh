#!/usr/bin/env bash
# Stage 3: baseline an existing database that already has the full schema.
#
# 1. schema check (tables, enums, min column counts)
# 2. insert seaql_migrations rows for all versions without re-running DDL
#
# Usage:
#   DATABASE_URL=postgres://... ./scripts/baseline_existing_db.sh
#   ./scripts/baseline_existing_db.sh --force   # skip check (not recommended)

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

if [[ -z "${DATABASE_URL:-}" ]]; then
  echo "DATABASE_URL is required" >&2
  exit 1
fi

export DATABASE_URL

EXTRA=()
if [[ "${1:-}" == "--force" ]]; then
  EXTRA+=(--force)
fi

echo "baseline_existing_db: check + baseline"
exec "$ROOT/scripts/migrate.sh" baseline "${EXTRA[@]+"${EXTRA[@]}"}"
