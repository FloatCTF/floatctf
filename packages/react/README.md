# @floatctf/react

**Optional** headless React bindings for FloatCTF. A non-React frontend never needs this
package — it talks to `@floatctf/sdk` directly.

It contains **no UI**: no components, no Primer, no Tailwind, no CSS, no routes, no pages,
no navigation, no dialogs. Hooks return data / actions / state, never JSX.

## Usage

```ts
import { createFloatCTFReact } from "@floatctf/react";
import { createFloatCTFClient } from "@floatctf/sdk";

const client = createFloatCTFClient({
  baseUrl: "/api",
  getUserToken: () => myStore.getState().token,
  getAdminToken: () => myStore.getState().adminToken,
});

export const {
  // realtime: fetch-based SSE + polling fallback + React Query invalidation.
  // SSE URLs are derived from the client you passed in:
  //   useAwdEventStream / useAwdpEventStream / useAwdpRunStream → client.baseUrl
  //   useAdminAwdEventStream                                    → client.adminBaseUrl
  // so REST and realtime always hit the same configured origin (works with an
  // absolute cross-origin baseUrl such as "http://127.0.0.1:17780/api").
  useAwdEventStream,
  useAdminAwdEventStream,
  useAwdpEventStream,
  useAwdpRunStream,
  // query option factories (keys identical to the pre-split frontend)
  eventInfoQueryOptions,
  challengeQueryOptions,
  challengeInstanceQueryOptions,
  systemInformationQueryOptions,
  // shared invalidation helpers
  invalidateAwdQueries,
  AWD_PLAYER_QUERY_KEYS,
  AWD_ADMIN_QUERY_KEYS,
} = createFloatCTFReact({
  client,
  // Auth storage stays frontend-owned: hand over your own selectors.
  useUserToken: () => useAuthStore((s) => s.token),
  useAdminToken: () => useAuthStore((s) => s.adminToken),
});
```

| Exported | Kind |
|---|---|
| `createFloatCTFReact` | factory binding a client + token selectors |
| `useAwdEventStream` etc. | realtime data/state hooks |
| `*QueryOptions` | TanStack Query option factories |
| `invalidateAwdQueries`, `AWD_*_QUERY_KEYS` | pure invalidation helpers |
| `UseTokenSource` | the injected token-hook type |

Peer dependencies: `react@^19.0.0`, `@tanstack/react-query@^5.66.5`. Both must be installed
by the consuming application (see “Consuming it outside the monorepo” below).

Non-React frontends (Vue, Svelte, Solid, vanilla TypeScript) implement the same
`mount(context)` contract from `@floatctf/frontend-runtime` and use `@floatctf/sdk`
directly. See [`docs/frontend/DEVELOPING.md`](../../docs/frontend/DEVELOPING.md).

## Consuming it outside the monorepo (v1.0)

`@floatctf/react` is **not published to the npm registry for v1.0** — npm publication is a
separate, optional release step that has *not* been performed. External consumers install
the release tarballs plus the peer dependencies:

```bash
# 1. build the three release tarballs (also runs `pnpm run build:packages`)
scripts/package-sdk-dist.sh /tmp/floatctf-dist

# 2. verify them (SDK-SHA256SUMS lists `<sha256>  <name>`)
( cd /tmp/floatctf-dist && sha256sum -c SDK-SHA256SUMS )

# 3. install into a project outside this repository — always install @floatctf/sdk too,
#    because this package depends on it
npm install /tmp/floatctf-dist/floatctf-sdk-1.0.0.tgz \
            /tmp/floatctf-dist/floatctf-react-1.0.0.tgz
npm install react@^19 react-dom@^19 @tanstack/react-query@^5
```

Replace `1.0.0` with the release version (`<V>` in `floatctf-react-<V>.tgz`). The tarball
ships `dist/` (with `.d.ts`), `README.md` and the AGPL-3.0-only `LICENSE`; it contains no
source, tests or `node_modules`. Its `dependencies` entry for `@floatctf/sdk` is rewritten
by the packing step from the workspace protocol to a concrete version (`1.0.0`), never
`workspace:*`.

> **pnpm note.** `pnpm add` cannot install these interdependent tarballs while the
> `@floatctf/*` names are absent from the registry: it resolves this package's
> `"@floatctf/sdk": "1.0.0"` through the registry and fails with `ERR_PNPM_FETCH_404`, even
> when the SDK tarball is passed in the same command (reproducible with any two unpublished
> local tarballs). Use `npm install` for the tarball flow; after the packages are on a
> registry, `pnpm add @floatctf/sdk @floatctf/react` works normally.
> `scripts/test-sdk-dist.sh` proves the npm flow and reports the pnpm behaviour.

This package's `dist/index.d.ts` reproduces TanStack Query's `queryOptions()` branded
`queryKey` (the `dataTagSymbol` unique symbol), so type-checking it requires
`skipLibCheck: true` — the setting used by every tsconfig in this repository and the
TanStack Query default. With `skipLibCheck: false`, TypeScript reports
`Cannot find name 'dataTagSymbol'` inside the emitted declaration file.
