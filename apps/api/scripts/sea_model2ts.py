import re
import sys
from pathlib import Path


def to_pascal_case(name: str) -> str:
    return "".join(part.capitalize() for part in re.split(r"[_-]+", name) if part)


def parse_enum(rust_code: str):
    """
    解析 ActiveEnum → 返回 (ts_code, enum_names)
    """
    output_blocks = []
    enum_names = []

    enum_pattern = re.compile(r"pub\s+enum\s+(\w+)\s*\{([\s\S]*?)\}", re.MULTILINE)

    for enum_name, body in enum_pattern.findall(rust_code):
        variant_pattern = re.compile(
            r'sea_orm\(string_value\s*=\s*"([^"]+)"\)\]\s*\n\s*(\w+)', re.MULTILINE
        )

        variants = variant_pattern.findall(body)

        if not variants:
            continue

        enum_names.append(enum_name)
        lines = [f"  {var} = '{val}'," for val, var in variants]
        ts_block = f"export enum {enum_name} {{\n" + "\n".join(lines) + "\n}"
        output_blocks.append(ts_block)

    return "\n\n".join(output_blocks), enum_names


def rust_to_ts(rust_code: str, known_enums: set, enums_module: str):
    """
    把 struct 转换成 TS type
    """
    table_match = re.search(r'table_name\s*=\s*"([^"]+)"', rust_code)
    table_name = table_match.group(1) if table_match else None
    if not table_name:
        return ""

    type_name = to_pascal_case(table_name)

    field_pattern = re.compile(r"pub\s+(?:r#)?(\w+)\s*:\s*([^,]+),")
    lines = []
    imports = set()

    for name, rust_type in field_pattern.findall(rust_code):
        ts_type = "string"
        optional = False

        clean_type = rust_type.strip()
        # 判断 Option
        if re.search(r"Option\s*<\s*(\w+)\s*>", clean_type):
            inner = re.search(r"Option\s*<\s*(\w+)\s*>", clean_type).group(1)
            if inner in known_enums:
                ts_type = inner
                imports.add(inner)
            else:
                ts_type = "string"
            optional = True
        else:
            if clean_type in known_enums:
                ts_type = clean_type
                imports.add(clean_type)
            elif re.search(r"Uuid", clean_type):
                ts_type = "string"
            elif re.search(r"i32|i64|u32|u64|f32|f64", clean_type):
                ts_type = "number"
            elif re.search(r"String", clean_type):
                ts_type = "string"
            elif re.search(r"bool", clean_type):
                ts_type = "boolean"
            else:
                ts_type = "string"

        lines.append(f"  {name}{'?' if optional else ''}: {ts_type};")

    if not lines:
        return ""

    import_line = ""
    if imports:
        import_line = (
            f"import type {{ {', '.join(sorted(imports))} }} from './{enums_module}';\n\n"
        )

    return import_line + f"export type {type_name} = {{\n" + "\n".join(lines) + "\n};\n"


def convert_directory(input_dir: str, output_dir: str):
    in_path = Path(input_dir)
    out_path = Path(output_dir)
    out_path.mkdir(parents=True, exist_ok=True)

    exports = []

    known_enums = set()
    enums_module_name = None

    # 先找枚举文件
    for rust_file in in_path.glob("*.rs"):
        if "enum" in rust_file.name:  # 简单判断，或根据实际文件名筛选
            rust_code = rust_file.read_text(encoding="utf-8")
            ts_code, enum_names = parse_enum(rust_code)
            if ts_code.strip():
                ts_filename = rust_file.stem + ".ts"
                (out_path / ts_filename).write_text(ts_code, encoding="utf-8")
                print(f"已转换枚举: {rust_file.name} → {ts_filename}")
                exports.append(rust_file.stem)
                known_enums.update(enum_names)
                enums_module_name = rust_file.stem

    # 再处理 struct
    for rust_file in in_path.glob("*.rs"):
        if "enum" in rust_file.name:
            continue

        rust_code = rust_file.read_text(encoding="utf-8")
        ts_code = rust_to_ts(rust_code, known_enums, enums_module_name or "")
        if not ts_code.strip():
            print(f"跳过: {rust_file.name}")
            continue

        ts_filename = rust_file.stem + ".ts"
        (out_path / ts_filename).write_text(ts_code, encoding="utf-8")
        print(f"已转换结构体: {rust_file.name} → {ts_filename}")
        exports.append(rust_file.stem)

    # 生成 index.ts
    if exports:
        index_lines = [f'export * from "./{name}";' for name in exports]
        (out_path / "index.ts").write_text(
            "\n".join(index_lines) + "\n", encoding="utf-8"
        )
        print(f"已生成: {out_path / 'index.ts'}")
    else:
        print("⚠️ 没有生成任何 TS 文件。")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("用法: python rust2ts.py <输入目录> <输出目录>")
        sys.exit(1)

    convert_directory(sys.argv[1], sys.argv[2])
