#!/usr/bin/env python3

from __future__ import annotations

import re
import sys
from pathlib import Path


# ==============================================================================
# Paths
# ==============================================================================

SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parent

INPUT_DIR = PROJECT_ROOT / "apps" / "api" / "src" / "entity"
OUTPUT_DIR = PROJECT_ROOT / "apps" / "web" / "src" / "entity"

# Generated TS types represent DATABASE columns only.
# API computed fields (e.g. Settings.resolved_value) live in manual DTO files
# under apps/web/src/api/, outside this generated directory.

SKIP_RUST_STEMS = {
    "mod",
    "prelude",
}


# ==============================================================================
# Helpers
# ==============================================================================


def die(message: str, code: int = 1) -> None:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(code)


def to_pascal_case(name: str) -> str:
    return "".join(
        part.capitalize()
        for part in re.split(r"[_-]+", name)
        if part
    )


# ==============================================================================
# Enum
# ==============================================================================


def parse_enum(rust_code: str):
    """
    解析 ActiveEnum。

    返回：
        (ts_code, enum_names)

    保持原脚本行为不变。
    """

    output_blocks = []
    enum_names = []

    enum_pattern = re.compile(
        r"pub\s+enum\s+(\w+)\s*\{([\s\S]*?)\}",
        re.MULTILINE,
    )

    for enum_name, body in enum_pattern.findall(rust_code):
        variant_pattern = re.compile(
            r'sea_orm\(string_value\s*=\s*"([^"]+)"\)\]\s*\n\s*(\w+)',
            re.MULTILINE,
        )

        variants = variant_pattern.findall(body)

        if not variants:
            continue

        enum_names.append(enum_name)

        lines = [
            f"  {var} = '{val}',"
            for val, var in variants
        ]

        ts_block = (
            f"export enum {enum_name} {{\n"
            + "\n".join(lines)
            + "\n}"
        )

        output_blocks.append(ts_block)

    return "\n\n".join(output_blocks), enum_names


# ==============================================================================
# Rust Model -> TypeScript
# ==============================================================================


def rust_to_ts(
    rust_code: str,
    known_enums: set[str],
    enums_module: str,
) -> str:
    """
    把 SeaORM entity struct 转换成 TS type。

    保持原脚本的类型映射逻辑不变。
    """

    table_match = re.search(
        r'table_name\s*=\s*"([^"]+)"',
        rust_code,
    )

    table_name = (
        table_match.group(1)
        if table_match
        else None
    )

    if not table_name:
        return ""

    type_name = to_pascal_case(table_name)

    field_pattern = re.compile(
        r"pub\s+(?:r#)?(\w+)\s*:\s*([^,]+),"
    )

    lines = []
    imports = set()

    for name, rust_type in field_pattern.findall(rust_code):
        ts_type = "string"
        optional = False

        clean_type = rust_type.strip()

        # ======================================================================
        # Option<T>
        #
        # 保持原行为：
        #
        # Option<String>
        # ->
        # field?: string;
        #
        # 而不是：
        # field: string | null;
        # ======================================================================

        option_match = re.search(
            r"Option\s*<\s*(\w+)\s*>",
            clean_type,
        )

        if option_match:
            inner = option_match.group(1)

            if inner in known_enums:
                ts_type = inner
                imports.add(inner)
            else:
                ts_type = "string"

            optional = True

        else:
            # ==================================================================
            # ActiveEnum
            # ==================================================================

            if clean_type in known_enums:
                ts_type = clean_type
                imports.add(clean_type)

            # ==================================================================
            # UUID
            # ==================================================================

            elif re.search(r"Uuid", clean_type):
                ts_type = "string"

            # ==================================================================
            # Number
            #
            # 保持原行为：
            # i32 / i64 / u32 / u64 / f32 / f64 -> number
            # ==================================================================

            elif re.search(
                r"i32|i64|u32|u64|f32|f64",
                clean_type,
            ):
                ts_type = "number"

            # ==================================================================
            # String
            # ==================================================================

            elif re.search(r"String", clean_type):
                ts_type = "string"

            # ==================================================================
            # Boolean
            # ==================================================================

            elif re.search(r"bool", clean_type):
                ts_type = "boolean"

            # ==================================================================
            # Fallback
            #
            # 保持原行为：
            # 未识别类型统一使用 string
            # ==================================================================

            else:
                ts_type = "string"

        lines.append(
            f"  {name}{'?' if optional else ''}: {ts_type};"
        )

    if not lines:
        return ""

    # ==========================================================================
    # Enum imports
    #
    # 保持原行为：import type
    # ==========================================================================

    import_line = ""

    if imports:
        import_line = (
            f"import type {{ {', '.join(sorted(imports))} }} "
            f"from './{enums_module}';\n\n"
        )

    return (
        import_line
        + f"export type {type_name} = {{\n"
        + "\n".join(lines)
        + "\n};\n"
    )


# ==============================================================================
# Conversion
# ==============================================================================


def reset_output_dir(output_dir: Path) -> None:
    """
    Rebuild generated TS entity types from zero.

    Manual API DTOs must NOT live under this directory — they would be wiped.
    """

    if output_dir.exists():
        if not output_dir.is_dir():
            die(f"output path exists but is not a directory: {output_dir}")
        for child in output_dir.iterdir():
            if child.is_file():
                child.unlink()
            elif child.is_dir():
                # entity/ is flat generated files only; refuse nested leftovers.
                die(
                    f"unexpected subdirectory in generated entity types: {child}\n"
                    "manual types must live outside apps/web/src/entity/"
                )
    output_dir.mkdir(parents=True, exist_ok=True)


def convert_directory(
    input_dir: Path,
    output_dir: Path,
) -> None:
    if not input_dir.is_dir():
        die(f"input directory not found: {input_dir}")

    exports = []

    known_enums: set[str] = set()
    enums_module_name: str | None = None

    rust_files = sorted(
        path
        for path in input_dir.glob("*.rs")
        if path.stem not in SKIP_RUST_STEMS
    )

    if not rust_files:
        die(f"no .rs files found in: {input_dir}")

    print("=" * 96)
    print("FloatCTF Web Type Generator")
    print("=" * 96)
    print()
    print(f"Input  : {input_dir}")
    print(f"Output : {output_dir}")
    print(f"Files  : {len(rust_files)}")
    print()
    print("Note   : generated types = DB columns only; API computed fields live in apps/web/src/api/")
    print()
    print("=" * 96)
    print()

    reset_output_dir(output_dir)

    # ==========================================================================
    # 第一遍：处理枚举
    #
    # 保持原行为：
    # 文件名中包含 "enum" 才被认为是枚举文件。
    # ==========================================================================

    for rust_file in rust_files:
        if "enum" not in rust_file.name:
            continue

        rust_code = rust_file.read_text(
            encoding="utf-8"
        )

        ts_code, enum_names = parse_enum(
            rust_code
        )

        if not ts_code.strip():
            continue

        ts_filename = rust_file.stem + ".ts"
        output_file = output_dir / ts_filename

        output_file.write_text(
            ts_code,
            encoding="utf-8",
        )

        print(
            f"已转换枚举: "
            f"{rust_file.name} → {ts_filename}"
        )

        exports.append(
            rust_file.stem
        )

        known_enums.update(
            enum_names
        )

        # 保持原行为：
        # 只保存最后一个枚举模块名。
        enums_module_name = rust_file.stem

    # ==========================================================================
    # 第二遍：处理 struct
    # ==========================================================================

    for rust_file in rust_files:
        if "enum" in rust_file.name:
            continue

        rust_code = rust_file.read_text(
            encoding="utf-8"
        )

        ts_code = rust_to_ts(
            rust_code,
            known_enums,
            enums_module_name or "",
        )

        if not ts_code.strip():
            print(
                f"跳过: {rust_file.name}"
            )
            continue

        ts_filename = rust_file.stem + ".ts"
        output_file = output_dir / ts_filename

        output_file.write_text(
            ts_code,
            encoding="utf-8",
        )

        print(
            f"已转换结构体: "
            f"{rust_file.name} → {ts_filename}"
        )

        exports.append(
            rust_file.stem
        )

    # Guard: infrastructure metadata must never become a web entity type.
    leaked = sorted(output_dir.glob("schema_migrations.ts"))
    if leaked:
        die(
            "schema_migrations.ts was generated; infrastructure tables must be "
            "excluded by gen_entities.py before web type generation"
        )

    # ==========================================================================
    # index.ts
    # ==========================================================================

    if exports:
        index_lines = [
            f'export * from "./{name}";'
            for name in exports
        ]

        index_file = output_dir / "index.ts"

        index_file.write_text(
            "\n".join(index_lines) + "\n",
            encoding="utf-8",
        )

        print()
        print(
            f"已生成: {index_file}"
        )

    else:
        print(
            "⚠️ 没有生成任何 TS 文件。"
        )

    # ==========================================================================
    # Summary
    # ==========================================================================

    generated = sorted(
        output_dir.glob("*.ts")
    )

    print()
    print("=" * 96)
    print("Web types generation completed")
    print("=" * 96)
    print()
    print(f"Generated files : {len(generated)}")
    print(f"Generated at    : {output_dir}")


# ==============================================================================
# Main
# ==============================================================================


def main() -> None:
    convert_directory(
        INPUT_DIR,
        OUTPUT_DIR,
    )


if __name__ == "__main__":
    main()
