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
