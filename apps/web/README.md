# @floatctf/web — FloatCTF web bootstrap

This package is **not** a UI. It is the tiny host page that:

1. reads the browser-local break-glass override (`?frontend=<id>`),
2. calls `bootstrapFrontend()` from `@floatctf/frontend-runtime`,
3. which fetches `GET /api/frontend` + the local frontend registry, validates
   compatibility, injects the frontend's styles and dynamically imports its ESM entry,
4. falls back to the Default Frontend, and finally renders a built-in emergency page.

The real UI lives in [`frontends/default`](../../frontends/default) and is installed
under `$FLOATCTF_HOME/frontends/`.

Deliberately React-free, Primer-free and stable. See
[`docs/frontend/ARCHITECTURE.md`](../../docs/frontend/ARCHITECTURE.md).

## License
本项目以 [GNU AGPLv3](LICENSE) 协议发布。
Copyright (C) 2025-2026 fb0sh@outlook.com
