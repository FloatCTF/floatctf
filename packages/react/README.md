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

Peer dependencies: `react`, `@tanstack/react-query`.

Non-React frontends (Vue, Svelte, Solid, vanilla TypeScript) implement the same
`mount(context)` contract from `@floatctf/frontend-runtime` and use `@floatctf/sdk`
directly. See [`docs/frontend/DEVELOPING.md`](../../docs/frontend/DEVELOPING.md).
