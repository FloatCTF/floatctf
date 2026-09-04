#!/usr/bin/env python3

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit


# ==============================================================================
# Paths
# ==============================================================================

SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parent

API_DIR = PROJECT_ROOT / "apps" / "api"
ENTITY_DIR = API_DIR / "src" / "entity"

# Infrastructure metadata tables managed outside the domain model.
# Never generate SeaORM entities / Web types for these.
# sea-orm-cli default also ignores seaql_migrations; keep both explicit.
EXCLUDED_TABLES = (
    "schema_migrations",
    "seaql_migrations",
)


# ==============================================================================
# Helpers
# ==============================================================================

def die(message: str, code: int = 1) -> None:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(code)


def get_config_path() -> Path:
    """
    从环境变量 FLOATCTF_CONFIG 获取配置文件路径。

    相对路径按照项目根目录解析。
    """

    value = os.getenv("FLOATCTF_CONFIG")

    if not value or not value.strip():
        die(
            "FLOATCTF_CONFIG is not set\n\n"
            "example:\n"
            "  export FLOATCTF_CONFIG=config/dev.toml"
        )

    path = Path(value.strip()).expanduser()

    if not path.is_absolute():
        path = PROJECT_ROOT / path

    path = path.resolve()

    if not path.is_file():
        die(f"config file not found: {path}")

    return path


def load_config(path: Path) -> dict:
    """
    加载 TOML 配置文件。
    """

    try:
        with path.open("rb") as file:
            return tomllib.load(file)

    except tomllib.TOMLDecodeError as exc:
        die(f"failed to parse TOML: {exc}")

    except OSError as exc:
        die(f"failed to read config file: {exc}")


def get_database_url(config: dict) -> str:
    """
    从：

        [database]
        url = "..."

    读取数据库连接 URL。
    """

    try:
        url = config["database"]["url"]
    except (KeyError, TypeError):
        die(
            "missing database URL in config\n\n"
            "expected:\n"
            "  [database]\n"
            '  url = "postgresql://user:password@host:5432/database"'
        )

    if not isinstance(url, str):
        die("[database].url must be a string")

    url = url.strip()

    if not url:
        die("[database].url is empty")

    return url


def mask_database_url(url: str) -> str:
    """
    隐藏数据库 URL 中的密码，避免输出到终端。
    """

    try:
        parts = urlsplit(url)

        if parts.password is None:
            return url

        hostname = parts.hostname or ""

        if parts.port is not None:
            host = f"{hostname}:{parts.port}"
        else:
            host = hostname

        if parts.username:
            netloc = f"{parts.username}:***@{host}"
        else:
            netloc = host

        return urlunsplit(
            (
                parts.scheme,
                netloc,
                parts.path,
                parts.query,
                parts.fragment,
            )
        )

    except Exception:
        return "<hidden>"


def find_sea_orm_cli() -> str:
    """
    查找 sea-orm-cli。
    """

    executable = shutil.which("sea-orm-cli")

    if executable is None:
        die(
            "sea-orm-cli not found\n\n"
            "install it with:\n"
            "  cargo install sea-orm-cli --version 1.1.20 --locked "
        )

    return executable


def remove_old_entities() -> None:
    """
    删除旧的 entity 目录。

    这样可以避免数据库删除表后，旧 entity 文件继续残留。
    """

    if not ENTITY_DIR.exists():
        return

    if not ENTITY_DIR.is_dir():
        die(f"entity path exists but is not a directory: {ENTITY_DIR}")

    print(f"Removing old entities:")
    print(f"  {ENTITY_DIR}")
    print()

    shutil.rmtree(ENTITY_DIR)


# ==============================================================================
# Generator
# ==============================================================================

def generate_entities(database_url: str) -> None:
    """
    调用 sea-orm-cli 根据数据库当前 schema 生成 Entity。
    """

    if not API_DIR.is_dir():
        die(f"API directory not found: {API_DIR}")

    sea_orm_cli = find_sea_orm_cli()

    remove_old_entities()

    ENTITY_DIR.mkdir(parents=True, exist_ok=True)

    ignore_tables = ",".join(EXCLUDED_TABLES)

    command = [
        sea_orm_cli,
        "generate",
        "entity",
        "-u",
        database_url,
        "-o",
        "src/entity",
        "--with-serde",
        "both",
        "--enum-extra-attributes",
        'serde(rename_all = "snake_case")',
        # Official filter: infrastructure metadata is not a domain entity.
        "--ignore-tables",
        ignore_tables,
    ]

    print("=" * 96)
    print("Generating SeaORM Entities")
    print("=" * 96)
    print()
    print("Command:")
    print(
        "  sea-orm-cli generate entity "
        "-o src/entity "
        "--with-serde both "
        '--enum-extra-attributes \'serde(rename_all = "snake_case")\' '
        f"--ignore-tables {ignore_tables}"
    )
    print()
    print(f"Excluded tables: {', '.join(EXCLUDED_TABLES)}")
    print()

    try:
        subprocess.run(
            command,
            cwd=API_DIR,
            check=True,
        )

    except subprocess.CalledProcessError as exc:
        die(
            f"sea-orm-cli failed with exit code {exc.returncode}",
            exc.returncode,
        )

    assert_excluded_entities_absent()


# ==============================================================================
# CIDR/INET 列类型修正（sqlx 兼容）
# ==============================================================================
# SeaORM 1.1.20 对 PostgreSQL 原生 cidr/inet 列生成 `custom("cidr"/"inet")` +
# String 字段；但 sqlx 无法把 cidr/inet 的返回值解码成 String（ColumnDecode
# 错误，2026-08-08 实证）。启用 sea-orm `with-ipnetwork` feature 后解码路径
# 是 ipnetwork::IpNetwork，因此这里把对应字段类型改为 IpNetwork，保证
# INSERT/SELECT 均可用（DoD #13：DB 使用原生 CIDR/INET）。
#
# 该转换只作用于 `#[sea_orm(column_type = "custom("inet")"/"custom("cidr")")]`
# 标记的 String 字段；其余字段原样保留。每次 db:gen 后自动执行，保证实体
# 仍由生成工具产出（不手工改实体）。
# ==============================================================================

IP_COLUMN_TYPES = ('\\"inet\\"', '\\"cidr\\"')


def postprocess_ip_columns() -> None:
    for entity_file in sorted(ENTITY_DIR.glob("*.rs")):
        if entity_file.name in ("mod.rs", "prelude.rs", "sea_orm_active_enums.rs"):
            continue
        text = entity_file.read_text(encoding="utf-8")
        # 实体源码里 custom("inet") 以转义形式 custom(\"inet\") 出现
        if 'custom(' not in text or ('inet' not in text and 'cidr' not in text):
            continue
        lines = text.splitlines(keepends=True)
        out: list[str] = []
        pending_ip = False
        for line in lines:
            if 'column_type = "custom(' in line and any(
                t in line for t in IP_COLUMN_TYPES
            ):
                pending_ip = True
                out.append(line)
                continue
            if pending_ip:
                pending_ip = False
                stripped = line.strip()
                if stripped.startswith("pub "):
                    # pub xxx: String,  ->  pub xxx: IpNetwork,
                    head, _, rest = line.partition(":")
                    out.append(head + ": IpNetwork," + "\n")
                    continue
            out.append(line)
        text = "".join(out)
        if "IpNetwork" in text and "use ipnetwork::IpNetwork" not in text:
            # Deterministic import placement to keep db:gen:rs idempotent:
            # after last `use super::...`, else before the first other `use`.
            lines = text.splitlines(keepends=True)
            last_super: int | None = None
            first_use: int | None = None
            for i, ln in enumerate(lines):
                if not ln.startswith("use "):
                    continue
                if first_use is None:
                    first_use = i
                if ln.startswith("use super::"):
                    last_super = i
            if last_super is not None:
                insert_at = last_super + 1
            elif first_use is not None:
                insert_at = first_use
            else:
                insert_at = 0
            lines.insert(insert_at, "use ipnetwork::IpNetwork;\n")
            text = "".join(lines)
        entity_file.write_text(text, encoding="utf-8")


def assert_excluded_entities_absent() -> None:
    """
    Fail hard if an infrastructure metadata table leaked into entity output.
    """

    leaked: list[str] = []

    for table in EXCLUDED_TABLES:
        entity_file = ENTITY_DIR / f"{table}.rs"
        if entity_file.exists():
            leaked.append(str(entity_file))

        mod_rs = ENTITY_DIR / "mod.rs"
        if mod_rs.is_file():
            mod_text = mod_rs.read_text(encoding="utf-8")
            if re_search_mod(table, mod_text):
                leaked.append(f"{mod_rs} mentions {table}")

        prelude_rs = ENTITY_DIR / "prelude.rs"
        if prelude_rs.is_file():
            prelude_text = prelude_rs.read_text(encoding="utf-8")
            if table in prelude_text:
                leaked.append(f"{prelude_rs} mentions {table}")

    if leaked:
        details = "\n  - ".join(leaked)
        die(
            "excluded infrastructure table leaked into SeaORM entities:\n"
            f"  - {details}\n\n"
            f"EXCLUDED_TABLES = {list(EXCLUDED_TABLES)}"
        )


def re_search_mod(table: str, mod_text: str) -> bool:
    # Match `pub mod schema_migrations;` / `schema_migrations::Entity` only.
    return re.search(rf"\b{re.escape(table)}\b", mod_text) is not None

# ==============================================================================
# Main
# ==============================================================================

def main() -> None:
    config_path = get_config_path()
    config = load_config(config_path)
    database_url = get_database_url(config)

    print("=" * 96)
    print("FloatCTF SeaORM Entity Generator")
    print("=" * 96)
    print()
    print(f"Project root  : {PROJECT_ROOT}")
    print(f"Config        : {config_path}")
    print(f"Database      : {mask_database_url(database_url)}")
    print(f"Entity output : {ENTITY_DIR}")
    print()
    print("=" * 96)
    print()

    generate_entities(database_url)
    postprocess_ip_columns()

    print()
    print("=" * 96)
    print("SeaORM entities generated successfully")
    print("=" * 96)
    print()
    print(f"Generated at:")
    print(f"  {ENTITY_DIR}")


if __name__ == "__main__":
    main()
