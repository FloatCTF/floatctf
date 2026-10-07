# @floatctf/frontend-runtime

FloatCTF pluggable frontend **artifact / runtime contract**. Framework-independent:
it does not depend on React, Vue, Svelte, Primer, a router, or a state library.

- `frontend.json` manifest schema + strict validation (`parseFrontendManifest`)
- installed-frontend registry schema + resolution (`parseRegistry`, `resolveFrontend`)
- runtime module contract (`FloatCTFFrontendModule.mount(context)`)
- the bootstrap loader with break-glass fallback (`bootstrapFrontend`)

Docs: [`docs/frontend/ARCHITECTURE.md`](../../docs/frontend/ARCHITECTURE.md),
[`docs/frontend/ARTIFACT.md`](../../docs/frontend/ARTIFACT.md).

```ts
import { bootstrapFrontend } from "@floatctf/frontend-runtime";

await bootstrapFrontend({ root: document.getElementById("app")! });
```

Contract versions are independent constants:

| constant | meaning |
|---|---|
| `FRONTEND_RUNTIME_VERSION` | `mount(context)` / `frontend.json` contract major |
| `API_CONTRACT_VERSION` | frontend-visible HTTP API contract major |
| `FRONTEND_REGISTRY_SCHEMA_VERSION` | local `registry.json` schema version |
| `FRONTEND_MANIFEST_SCHEMA_VERSION` | `frontend.json` document schema version |

> 要构建一个**完整的第三方 Frontend**，请读
> [docs/frontend/AI-FRONTEND-GUIDE.md](../../docs/frontend/AI-FRONTEND-GUIDE.md)
> （能力清单与完整性口径见 [CAPABILITY-MATRIX.md](../../docs/frontend/CAPABILITY-MATRIX.md)）。

## Consuming it outside the monorepo (v1.0)

`@floatctf/frontend-runtime` is **not published to the npm registry for v1.0** — npm
publication is a separate, optional release step that has *not* been performed. External
consumers install the release tarball:

```bash
# 1. build the three release tarballs (also runs `pnpm run build:packages`)
scripts/package-sdk-dist.sh /tmp/floatctf-dist

# 2. verify them (SDK-SHA256SUMS lists `<sha256>  <name>`)
( cd /tmp/floatctf-dist && sha256sum -c SDK-SHA256SUMS )

# 3. install into a project outside this repository
npm install /tmp/floatctf-dist/floatctf-frontend-runtime-1.0.0.tgz
```

Replace `1.0.0` with the release version (`<V>` in `floatctf-frontend-runtime-<V>.tgz`).
This package has **no runtime dependencies and no peer dependencies** — the tarball is
fully self-contained. It ships `dist/` (with `.d.ts`), `README.md` and the AGPL-3.0-only
`LICENSE`; it contains no source, tests or `node_modules`.

> **pnpm note.** When installing it together with the other two `@floatctf/*` tarballs,
> `pnpm add` fails with `ERR_PNPM_FETCH_404` while those names are absent from the registry
> (pnpm resolves `@floatctf/react`'s `@floatctf/sdk` dependency through the registry). Use
> `npm install` for the tarball flow; after registry publication, `pnpm add` works normally.
