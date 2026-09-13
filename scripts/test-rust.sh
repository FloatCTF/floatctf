#!/usr/bin/env bash
set -Eeuo pipefail

# Run Rust tests against a disposable sibling PostgreSQL database whenever the
# normal local development database is reachable. This prevents DB-backed
# integration tests from polluting floatctf_db while preserving their existing
# soft-skip behaviour on hosts that do not have PostgreSQL available.
#
# An explicit DATABASE_URL is treated as caller-owned (CI/custom harness) and
# is respected verbatim.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

log() { printf '[rust-test] %s\n' "$*" >&2; }

if [[ -n "${DATABASE_URL:-}" ]]; then
    log "DATABASE_URL supplied by caller; using caller-owned test database"
    exec cargo test --workspace "$@"
fi

for cmd in python3 psql createdb dropdb; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        log "$cmd unavailable; running tests with their normal DB-unavailable soft-skip behaviour"
        exec cargo test --workspace "$@"
    fi
done

BASE_CONFIG="${FLOATCTF_CONFIG:-$ROOT/apps/api/config/development.toml}"
if [[ ! -f "$BASE_CONFIG" ]]; then
    log "config not found at $BASE_CONFIG; running tests without DB isolation"
    exec cargo test --workspace "$@"
fi

BASE_URL="$(python3 - "$BASE_CONFIG" <<'PY'
import sys, tomllib
with open(sys.argv[1], 'rb') as fh:
    cfg = tomllib.load(fh)
print(cfg['database']['url'])
PY
)" || {
    log "cannot read [database].url; running tests without DB isolation"
    exec cargo test --workspace "$@"
}

if ! psql -X -q -A -t -v ON_ERROR_STOP=1 -d "$BASE_URL" -c 'SELECT 1' >/dev/null 2>&1; then
    log "PostgreSQL is unreachable; running tests with DB-backed suites soft-skipped"
    exec cargo test --workspace "$@"
fi

SUFFIX="$(date +%s)_$$"
DB_NAME="floatctf_test_${SUFFIX}"
TMP="$(mktemp -d "/tmp/floatctf-rust-test-${SUFFIX}.XXXXXX")"
TEST_CONFIG="$TMP/test.toml"
TEST_URL="$(python3 - "$BASE_URL" "$DB_NAME" <<'PY'
import sys
from urllib.parse import urlsplit, urlunsplit
u=urlsplit(sys.argv[1])
print(urlunsplit((u.scheme, u.netloc, '/' + sys.argv[2], u.query, u.fragment)))
PY
)"

cleanup() {
    local rc=$?
    set +e
    dropdb --maintenance-db="$BASE_URL" --if-exists --force "$DB_NAME" >/dev/null 2>&1 || true
    rm -rf "$TMP"
    if (( rc == 0 )); then
        log "PASS: disposable database $DB_NAME removed"
    else
        log "FAIL: tests exited with $rc; disposable database cleanup attempted"
    fi
    exit "$rc"
}
trap cleanup EXIT INT TERM

log "creating disposable PostgreSQL database $DB_NAME"
createdb --maintenance-db="$BASE_URL" "$DB_NAME"
cp "$BASE_CONFIG" "$TEST_CONFIG"
python3 - "$TEST_CONFIG" "$TEST_URL" <<'PY'
from pathlib import Path
import sys
p=Path(sys.argv[1]); url=sys.argv[2]
lines=p.read_text().splitlines()
in_database=False
changed=False
for i,line in enumerate(lines):
    stripped=line.strip()
    if stripped.startswith('[') and stripped.endswith(']'):
        in_database = stripped == '[database]'
        continue
    if in_database and stripped.startswith('url') and '=' in line:
        indent=line[:len(line)-len(line.lstrip())]
        lines[i]=f'{indent}url = "{url}"'
        changed=True
        break
if not changed:
    raise SystemExit('failed to rewrite [database].url')
p.write_text('\n'.join(lines)+'\n')
PY

log "applying migrations to disposable database"
FLOATCTF_CONFIG="$TEST_CONFIG" apps/api/src/sql/migrate.sh apply >/dev/null

# Migrations intentionally contain schema only; production/default settings are
# seeded by API bootstrap. DB-backed integration tests call service code directly,
# so mirror bootstrap's defaults in this script-owned disposable database. Do not
# do this for caller-owned DATABASE_URL databases (those return above unchanged).
TEST_SETTINGS_SQL="$TMP/seed-settings.sql"
python3 - "$TEST_CONFIG" "$TEST_SETTINGS_SQL" <<'PYSETTINGS'
from pathlib import Path
import sys, tomllib

config_path, out_path = sys.argv[1:]
with open(config_path, "rb") as fh:
    cfg = tomllib.load(fh)

def q(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"

defaults = [
    ("INSTANCE_DESTROY_DELAY", "60", "integer", "实例销毁延迟时间 (分钟)"),
    ("EVENT_SCORE_DECAY", "500", "integer", "比赛题目分数衰减系数"),
    ("EVENT_SCORE_MIN_PERCENT", "0.45", "float", "比赛题目最低分数为题目的百分比"),
    ("WORK_DIR", cfg["server"]["work_dir"], "string", "工作目录（其他设置可用 {{WORK_DIR}} 引用）"),
    ("CHALLENGES_DIR", "{{WORK_DIR}}/challenges", "string", "题目位置（支持 {{WORK_DIR}} 等变量引用）"),
    ("GAMEBOXES_DIR", "{{WORK_DIR}}/gameboxes", "string", "GameBox 位置（支持 {{WORK_DIR}} 等变量引用）"),
    ("HTTP_PREFIX", "http://", "string", "HTTP前缀"),
    ("NODE_IP", "127.0.0.1", "string", "节点IP"),
    ("FLAG_PREFIX", "flag", "string", "全局flag前缀"),
    ("MAIN_URL", cfg["application"]["main_url"], "string", "主站地址前缀baseURL"),
    ("SMTP_URI", "smtp.example.com:user@example.com:SMTP_PASS", "string", "SMTP服务器地址与凭证"),
]
values = ",\n".join(
    f"({q(k)}, {q(v)}, {q(t)}::public.setting_value_type, {q(d)})"
    for k, v, t, d in defaults
)
Path(out_path).write_text(
    "INSERT INTO public.settings (key, value, type, description) VALUES\n"
    + values
    + "\nON CONFLICT (key) DO NOTHING;\n"
)
PYSETTINGS
psql -X -q -v ON_ERROR_STOP=1 -d "$TEST_URL" -f "$TEST_SETTINGS_SQL" >/dev/null
[[ "$(psql -X -q -A -t -v ON_ERROR_STOP=1 -d "$TEST_URL" -c 'SELECT count(*) FROM settings')" -ge 11 ]] || {
    log "failed to seed bootstrap defaults into disposable database"
    exit 1
}
log "seeded bootstrap defaults into disposable database"

log "running cargo test --workspace against isolated database"
DATABASE_URL="$TEST_URL" FLOATCTF_CONFIG="$TEST_CONFIG" cargo test --workspace "$@"
