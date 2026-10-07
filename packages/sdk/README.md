# @floatctf/sdk

FloatCTF platform client SDK — **framework independent**. No React, no router, no state
library, no Primer, no Tailwind, no application pages.

```
FloatCTF Backend ── REST / SSE / Bearer ──▶ @floatctf/sdk ──▶ @floatctf/react ──▶ Frontend
```

## Usage

```ts
import { createFloatCTFClient } from "@floatctf/sdk";

const client = createFloatCTFClient({
  baseUrl: "/api",
  // Auth storage is FRONTEND-owned; the SDK only asks for a token.
  getUserToken: () => myStore.getState().token,
  getAdminToken: () => myStore.getState().adminToken,
  // What happens after a 401 is FRONTEND-owned: the SDK never navigates.
  onUnauthorized: ({ scope }) => {
    if (scope === "admin") myStore.getState().removeAdminToken();
    else myStore.getState().removeToken();
  },
});

const events = await client.service.events.fetch({ limit: 20 });
const client2 = await client.admin.settings.fetch();
```

`client.service` / `client.admin` / `client.awd` / `client.awdp` are the domain façades,
each bound to **that instance's own** HTTP transport.

### Multi-client / instance isolation

There is no module-global "current transport" and no shared client singleton. Every
`createFloatCTFClient()` call returns a fully independent client (own base URL, own token
sources, own error hooks); creation order does not matter and there is nothing to unbind:

```ts
const a = createFloatCTFClient({ baseUrl: "https://a.example/api", getUserToken: () => "A" });
const b = createFloatCTFClient({ baseUrl: "https://b.example/api", getUserToken: () => "B" });

await b.service.events.fetch(); // → b.example with "B"
await a.service.events.fetch(); // → a.example with "A"  (still!)
```

`client.baseUrl` / `client.adminBaseUrl` are authoritative: `requestConfig` cannot smuggle a
`baseURL` (it is excluded at the type level and overridden at runtime).

The individual domain modules are exported as **factories**
(`createEventServiceApi(http)`, `createAwdPlayerApi(http)`, …) for advanced composition;
regular code should use `client.*`.

Realtime: use `client.sse.connect({ url: "/events/<id>/awd/stream" })` (player base URL +
user token) or `client.sse.connectAdmin({ ... })` (admin base URL + admin token). Relative
URLs resolve against the owning client's base URL, and the Bearer token always travels in the
`Authorization` header — never in the URL.

## Error model

Rejected values are always [`FloatCTFError`](./src/errors.ts), exposing:

| field | meaning |
|---|---|
| `httpStatus` | HTTP status, when there was a response |
| `code` | platform response code (`UniResponse.code`) |
| `platformMessage` | platform message (`UniResponse.message`) |
| `kind` | `http` \| `platform` \| `network` \| `unknown` |
| `unauthorized` | true for 401 |
| `original` | the original cause (usually the Axios error) |
| `response` | Axios-compatible `{ status, data, headers }` shape |
| `displayMessage` | best message to show a user |

`response` is kept so existing UI code reading `error.response?.data?.message` keeps
working unchanged — the typed fields are additive, not a replacement.

## Responses

`UniResponse<T>` = `{ code, message, data?, meta? }` is preserved verbatim from the
backend. The SDK does not invent a second envelope.

## Realtime (SSE)

`connectSse` / `client.sse.connect` use `fetch` + `ReadableStream`, so the Bearer token
travels in the `Authorization` header and **never** in the URL. Reconnect uses
exponential backoff with jitter, stops on 401/403, honours `Retry-After` on 429, and
supports `Last-Event-ID`.

## Entity types

Generated DB-column types live behind a separate entry point so they cannot be confused
with API DTOs:

```ts
import type { Events } from "@floatctf/sdk/entity";
```

## Contract version

`API_CONTRACT_VERSION` (see `docs/frontend/ARCHITECTURE.md`) lives with the runtime
contract in `@floatctf/frontend-runtime`. `scripts/check-architecture.sh` asserts the two
declarations never drift.

> 要构建一个**完整的第三方 Frontend**，请读
> [docs/frontend/AI-FRONTEND-GUIDE.md](../../docs/frontend/AI-FRONTEND-GUIDE.md)
> （能力清单与完整性口径见 [CAPABILITY-MATRIX.md](../../docs/frontend/CAPABILITY-MATRIX.md)）。

## Consuming it outside the monorepo (v1.0)

`@floatctf/sdk` is **not published to the npm registry for v1.0** — npm publication is a
separate, optional release step that has *not* been performed. External consumers install
the release tarball instead:

```bash
# 1. build the release tarballs (also runs `pnpm run build:packages`)
scripts/package-sdk-dist.sh /tmp/floatctf-dist

# 2. verify them (SDK-SHA256SUMS lists `<sha256>  <name>`)
( cd /tmp/floatctf-dist && sha256sum -c SDK-SHA256SUMS )

# 3. install into a project outside this repository
npm install /tmp/floatctf-dist/floatctf-sdk-1.0.0.tgz
```

Replace `1.0.0` with the release version (`<V>` in `floatctf-sdk-<V>.tgz`). The tarball
ships `dist/` (with `.d.ts`), `README.md` and the AGPL-3.0-only `LICENSE`; it contains no
source, tests or `node_modules`. Its one runtime dependency, `axios`, is installed from the
public registry, so the tarball install needs network access. Everything exported here —
including the `@floatctf/sdk/entity` entry point — resolves from the tarball alone.

> **pnpm note.** `pnpm add` cannot install the three interdependent tarballs while the
> `@floatctf/*` names are absent from the registry (it tries to resolve `@floatctf/react`'s
> `"@floatctf/sdk"` through the registry and fails with `ERR_PNPM_FETCH_404`). Use
> `npm install` for the tarball flow; once the packages *are* on a registry,
> `pnpm add @floatctf/sdk` works normally. `scripts/test-sdk-dist.sh` documents and
> reproduces both behaviours.
