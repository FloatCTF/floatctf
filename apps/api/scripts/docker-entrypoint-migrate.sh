#!/bin/sh
# Stage 4: apply schema migrations before starting the main process.
#
# Prefer the compiled `floatctf-migration` binary (production image).
# Fall back to `scripts/migrate.sh` when developing from a full source tree.
#
# Env:
#   DATABASE_URL     — required to migrate (otherwise warn and continue)
#   SKIP_MIGRATE=1   — skip migrations
#
# Usage (Dockerfile):
#   ENTRYPOINT ["/app/docker-entrypoint-migrate.sh"]
#   CMD ["/app/floatctf"]

set -eu

if [ "${SKIP_MIGRATE:-}" = "1" ]; then
  echo "docker-entrypoint-migrate: SKIP_MIGRATE=1 — not applying migrations"
elif [ -z "${DATABASE_URL:-}" ]; then
  echo "docker-entrypoint-migrate: DATABASE_URL unset — skip migrate" >&2
else
  if [ -x /app/floatctf-migration ]; then
    echo "docker-entrypoint-migrate: /app/floatctf-migration up"
    /app/floatctf-migration up
  elif [ -x "$(dirname "$0")/migrate.sh" ]; then
    echo "docker-entrypoint-migrate: scripts/migrate.sh up"
    "$(dirname "$0")/migrate.sh" up
  elif command -v cargo >/dev/null 2>&1 && [ -d migration ]; then
    echo "docker-entrypoint-migrate: cargo run --manifest-path migration/Cargo.toml -- up"
    (cd migration && cargo run --config 'build.rustc-wrapper=""' --quiet -- up)
  else
    echo "docker-entrypoint-migrate: no migrator binary found; cannot apply schema" >&2
    exit 1
  fi
fi

if [ "$#" -eq 0 ]; then
  echo "docker-entrypoint-migrate: no command given" >&2
  exit 1
fi

exec "$@"
