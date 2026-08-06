#!/usr/bin/env bash
# Apply or inspect FloatCTF schema migrations (base + AWD).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/migration"

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

CMD="${1:-up}"
shift || true

exec cargo run --config 'build.rustc-wrapper=""' --quiet -- "$CMD" "$@"
