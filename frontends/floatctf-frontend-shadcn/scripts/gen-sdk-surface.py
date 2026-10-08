import re, pathlib

ROOT = pathlib.Path("/home/fb0sh/Projects/floatctf/packages")
OUT = pathlib.Path("/home/fb0sh/Projects/floatctf/frontends/floatctf-frontend-shadcn/docs/dev/SDK-SURFACE.md")

sources = []
sources += sorted((ROOT / "sdk/src/api").rglob("*.ts"))
sources += [ROOT / "react/src/index.ts"]
sources += sorted((ROOT / "react/src").glob("use*.ts"))
sources += sorted((ROOT / "frontend-runtime/src").glob("*.ts"))

def blocks(text):
    """Yield (name, body) for `export interface X {` / `export type X =` declarations."""
    for m in re.finditer(r"export (interface|type) (\w+)([^\n]*?)(\{|=)", text):
        kind, name, between, tok = m.group(1), m.group(2), m.group(3), m.group(4)
        start = m.start()
        if tok == "=":
            # single-line type alias: take up to the terminating semicolon at depth 0
            i = m.end(); depth = 0
            while i < len(text):
                c = text[i]
                if c in "{([": depth += 1
                elif c in "})]": depth -= 1
                elif c == ";" and depth == 0:
                    break
                i += 1
            yield name, text[start:i + 1]
        else:
            i = m.end() - 1; depth = 0
            while i < len(text):
                if text[i] == "{": depth += 1
                elif text[i] == "}":
                    depth -= 1
                    if depth == 0:
                        break
                i += 1
            yield name, text[start:i + 1]

lines = ["# SDK 公共面速查（自动生成，勿手改）", "",
         "> 由 `scripts/gen-sdk-surface.py` 从 `packages/sdk/src/api/**`、`packages/react/src/**`、",
         "> `packages/frontend-runtime/src/**` 抽取真实声明生成。符号不存在就是不存在 —— **不要臆造**。",
         "> 逃生舱写法见 AI-FRONTEND-GUIDE §5.4：`client.serviceHttp` / `client.adminHttp` / `client.transport.*` / 原生 WebSocket。", ""]

for path in sources:
    if not path.is_file():
        continue
    text = path.read_text(encoding="utf-8")
    got = list(blocks(text))
    if not got:
        continue
    rel = path.relative_to(ROOT.parent)
    lines.append(f"## `{rel}`")
    lines.append("")
    for name, body in got:
        lines.append("```ts")
        lines.append(body.strip())
        lines.append("```")
        lines.append("")

OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
print("wrote", OUT, len(lines), "lines")
