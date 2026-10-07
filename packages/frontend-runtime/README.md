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
