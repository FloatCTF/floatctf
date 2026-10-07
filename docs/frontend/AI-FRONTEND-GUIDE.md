# AI 前端构建手册：创建一个全新的 FloatCTF Frontend

> **权威范围**：本文是**创建新 FloatCTF Frontend** 的权威作业口径。
> 制品字段、注册表 schema、路径安全规则与不可变版本策略以
> [ARTIFACT.md](./ARTIFACT.md) 为准；平台架构、依赖方向、信任模型与 `mount(context)`
> 运行时契约以 [ARCHITECTURE.md](./ARCHITECTURE.md) 为准；开发/安装/回滚流程以
> [DEVELOPING.md](./DEVELOPING.md) 为准。本文只**重述前端作者必须据此行动的部分**，
> 不复制上述文档的完整内容。
>
> 每项能力的公共 SDK 面、Default 参照页、是否实时、完整前端是否必需，见
> [CAPABILITY-MATRIX.md](./CAPABILITY-MATRIX.md)（与本文配套，创建前端时**必读**）。

---

## 1. 读者与适用范围

本文面向**在 FloatCTF 仓库之外创建新前端**的 AI 编码代理（Claude Code / Codex / Pi /
Cursor / 其它）以及驱动它们的人类。它同时适用于：

- 全新仓库：从零开始实现一个可插拔前端，通过制品安装进 FloatCTF；
- 全新前端目录：新增一个**不落在** `frontends/default/` 的前端实现。

本文**不**适用于「修改官方 Default Frontend」——那类任务看
[../agents/RULES.md](../agents/RULES.md) 与 §2.1。

契约常量速查（`packages/frontend-runtime/src/version.ts`）：
`FRONTEND_RUNTIME_VERSION = "1"`、`API_CONTRACT_VERSION = "1"`、
`FRONTEND_MANIFEST_SCHEMA_VERSION = 1`、`FRONTEND_REGISTRY_SCHEMA_VERSION = 1`、
`DEFAULT_FRONTEND_BASE_URL = "/__floatctf/frontends"`、`DEFAULT_API_BASE_URL = "/api"`、
`DEFAULT_FRONTEND_ID = "default"`。前端 `compatibility` 必须写这些 **major 字符串**。

---

## 2. 关键区分：修改 Default ≠ 创建新 Frontend

这是本手册最重要的一节。两类任务的约束**互斥**，混在一起必然返工。

| | 修改 `frontends/default` | 创建一个新 Frontend |
|---|---|---|
| 目标 | 让官方前端某个页面/流程**保持一致地**变化 | 一个**完整替换**的浏览器应用 |
| 视觉权威 | Default 现有 Primer / 布局 / 导航 / 组件就是权威 | 新前端**自主决定** |
| 允许复用 Default 源码 | 必须复用（同域参照页、既有 components） | **禁止依赖**（只可阅读参照） |
| 路由路径 | 沿用 Default 既有路由树 | 由新前端自定，平台不关心 |
| 框架 | 固定 React + TanStack Router/Query + Primer + Tailwind + Zustand | 任意框架 / 任意状态库 / 任意 CSS |

### 2.1 修改官方 Default Frontend

- 目标目录**只有** `frontends/default/`。
- Default 既有的 Primer、布局、导航、`GenericTable`、`Dialog`、路由结构、视觉约定
  **是权威**：保持一致，不要另起炉灶。
- 必须遵循 [../agents/RULES.md](../agents/RULES.md)，并对齐同域参照页
  （赛事详情参照 `frontends/default/src/routes/service/events/jeopardy.$id/*` 与
  `.../awd.$id/*`，管理列表参照 `frontends/default/src/routes/admin/challenges.tsx`，
  导航配置参照 `frontends/default/src/navigation/*`）。
- 优先复用 `frontends/default/src/components/` 既有组件。

### 2.2 创建新的可插拔 Frontend

- Default **仅仅**是以下内容的参照：受支持的业务功能、API 调用方式、鉴权语义、
  状态语义、错误场景、实时行为、边界情况，以及用户/管理员**有权看到什么信息**。
- Default **不是**以下内容的权威：配色、字体、组件、布局、页面层级、导航、路由路径、
  React、Primer、Tailwind、Zustand、TanStack Router、TanStack Query。这些全部由新前端
  自主决定。
- 读 Default 源码是可以的（见 §9）；**依赖**它是不允许的（见 §4）。
- **不得**为了创建新前端去修改 `frontends/default/`。若发现必须改后端才能继续，先确认
  这是**真实的公共契约缺口**，记录 `PUBLIC SDK GAP: <细节>` 并上报，不要擅自改后端。

### 2.3 路由路径示例：两者都合法

Default 使用
`/service/events/awd/$id/scoreboard`
（文件 `frontends/default/src/routes/service/events/awd.$id/scoreboard.tsx`，
`createFileRoute("/service/events/awd/$id/scoreboard")`）。

新前端完全可以使用 `/arena/$id`、`/workspace/$id`、`/event/$id/live` ——
后端不关心浏览器路由，`@floatctf/frontend-runtime` 也不关心
（见 [ARCHITECTURE.md §1](./ARCHITECTURE.md)）。

---

## 3. AI Agent 如何解读「前端」需求

**先判型，再动手。** 未判型就开始写代码是最常见的返工来源。

| 用户措辞 | 判定 | 目标 |
|---|---|---|
| 「修改官方前端」「改页面」「调整当前 UI」「修改 Default Frontend」 | **A** | `frontends/default/` |
| 「新建一个 frontend」「做一个新的前端」「写一个不同风格的 frontend」「创建 FloatCTF frontend」「做一个 cyberpunk / minimal / material frontend」 | **B** | 新的可插拔前端（新目录 / 外部仓库） |

规则：

1. 判定为 **B** 时**不得**修改 Default，除非用户显式要求。
2. 未限定的「创建一个 FloatCTF Frontend」= **完整替换前端**（覆盖
   [CAPABILITY-MATRIX.md](./CAPABILITY-MATRIX.md) 中 `required` 的全部能力），
   **不是**一个演示落地页（demo landing page）。
3. 用户明确要求部分范围（player-only / admin-only / scoreboard-only / kiosk）时，
   允许刻意部分实现；但 Agent **必须显式声明自己的覆盖范围**，并在
   `FRONTEND-PLAN.md` 里写成完整/部分（见 §10、§11）。
4. 判定有歧义时，**先问**，不要默认选 A。

---

## 4. 公共依赖边界

新前端**只允许**依赖以下 npm 包：

| 包 | 必需性 | 说明 |
|---|---|---|
| `@floatctf/sdk` | ✅ 必需 | 传输、领域 API、DTO、SSE、错误模型 |
| `@floatctf/frontend-runtime` | ✅ 必需 | `mount(context)` 契约、manifest 校验器、契约常量 |
| `@floatctf/react` | ⭕ 可选 | **仅** React 前端使用；headless、无 UI |
| 任意第三方库 | ✅ 允许 | Vue / Svelte / Solid / 路由 / 状态库 / CSS 方案 / 组件库任意 |

**MUST NOT** import：

- `apps/web/*`
- `frontends/default/*` 或 `frontends/default/src/*`
- `packages/*/src/*`（例如 `@floatctf/sdk/src/...`）
- `@/...` 或任何 FloatCTF monorepo 私有 alias
- 任何逃逸进 FloatCTF 仓库的相对路径（`../floatctf/...`）

**Default 源码是「示例即文档」，不是 API（Default source is documentation-by-example,
not an API）**：可以读，不可以依赖。依赖它会在下一次平台更新时静默损坏你的前端，
而且 `frontends/default` 的公开面不受任何兼容性承诺约束。

---

## 5. 三个包在 Agent 视角下的职责

### 5.1 `@floatctf/sdk`（框架无关，必需）

- **owns**：HTTP 传输（每实例独立的 axios 实例 + `Authorization: Bearer` 注入 +
  401 回调 + 错误归一化）、领域门面、视图无关 DTO 类型、fetch-based SSE、生成实体类型
  （独立入口 `@floatctf/sdk/entity`）。
- **does not own**：token 存储、导航/跳转、登录页、任何 UI、router、state library。
  SDK **绝不** import router / Zustand / `localStorage` / 任何登录页 URL。
- **primary usage**：

```ts
import { createFloatCTFClient } from "@floatctf/sdk";
const client = createFloatCTFClient({ baseUrl: "/api", getUserToken: () => token });
```

公开面（`packages/sdk/src/index.ts`）：

- 工厂与客户端：`createFloatCTFClient`、`FloatCTFClient`、`FloatCTFSseOptions`、
  `resolveSseUrl`、`createFloatCTFTransport`、`resolveBaseUrls`、`normalizeBaseUrl`、
  `DEFAULT_API_BASE_URL`、`FloatCTFClientOptions`、`FloatCTFTransport`、
  `FloatCTFHttpClient`、`FloatCTFRequestConfig`、`FloatCTFAuthScope`、`UnauthorizedContext`。
- 错误：`FloatCTFError`、`toFloatCTFError`、`floatCTFErrorFromEnvelope`、
  `FloatCTFErrorKind`、`FloatCTFErrorResponse`。
- 协议：`UNI_SUCCESS_CODE`、`UniResponse`、`QueryParams`。
- SSE：`connectSse`、`ConnectSseOptions`、`SseConnection`、`SseConnectionState`、
  `SseConnectionStatus`、`createSseParser`、`SseEvent`、`SseParser`。
- 领域：`client.service`（`ServiceApi`）、`client.admin`（`AdminApi`）、
  `client.awd.{player,admin}`、`client.awdp.{player,admin,runs}`；各领域模块同时以工厂
  形式导出（`createEventServiceApi(http)`、`createAwdPlayerApi(http)` 等）供高级组合。

**实例隔离**：没有模块级「当前 transport」，没有共享单例。`createFloatCTFClient()`
每次调用返回**完全独立**的客户端（自己的 base URL、token 来源、错误回调），创建顺序
无关，无需「解绑」。可以同时创建多个。

### 5.2 `@floatctf/frontend-runtime`（框架无关，必需）

- **owns**：制品 manifest schema 与校验（`parseFrontendManifest`、
  `describeManifestCompatibility`、`FloatCTFFrontendManifest`、
  `FloatCTFFrontendCompatibility`、`ManifestParseResult`）、注册表 schema 与解析
  （`parseRegistry`、`resolveFrontend`、`registerFrontendVersion`、`listFrontendIds`、
  `emptyRegistry`、`FloatCTFRegistry*`、`ResolvedFrontend`）、`mount(context)` 契约
  （`FloatCTFFrontendModule`、`FloatCTFMountContext`、`FloatCTFFrontendUnmount`、
  `FloatCTFFrontendImport`、`normalizeFrontendModule`）、bootstrap 与兜底 UI
  （`bootstrapFrontend`、`parseBootstrapInfo`、`BootstrapFrontendOptions`、
  `BootstrapFrontendResult`、`FloatCTFBootstrapInfo`、`renderBootstrapEmergencyUi`、
  `BootstrapDiagnostics`）、路径与兼容性原语（`checkRelativeAssetPath`、
  `isMajorCompatible`、`isSafeFrontendId`、`isValidSemver`、`joinUrlPath`、
  `parseMajorConstraint`、`FRONTEND_ID_PATTERN`、`FRONTEND_ID_MAX_LENGTH`）与契约常量。
- **does not own**：UI、框架、路由、状态库、API 调用。它**不**依赖 SDK，也**不**依赖任何
  UI 框架。
- **primary usage**：`import type { FloatCTFFrontendModule, FloatCTFMountContext }`；
  可选地在你的构建脚本里用 `parseFrontendManifest` 对生成的 `frontend.json` 自检
  （Default 的 Vite 插件就是这么做的，思路可照抄）。

### 5.3 `@floatctf/react`（可选，仅 React，headless）

- **owns**：`createFloatCTFReact({ client, useUserToken, useAdminToken? })` 工厂；
  它返回 4 个实时 hook（`useAwdEventStream`、`useAdminAwdEventStream`、
  `useAwdpEventStream`、`useAwdpRunStream`）、4 个 query options 工厂
  （`eventInfoQueryOptions`、`challengeQueryOptions`、`challengeInstanceQueryOptions`、
  `systemInformationQueryOptions`）、失效工具（`invalidateAwdQueries`、
  `AWD_PLAYER_QUERY_KEYS`、`AWD_ADMIN_QUERY_KEYS`）与 `UseTokenSource` 类型。
- **does not own**：任何 UI（组件 / Primer / Tailwind / CSS / 路由 / 页面 / 导航 / 对话框）。
  hook 只返回 data / actions / state，**从不返回 JSX**。
- peer dependencies：`react@^19.0.0`、`@tanstack/react-query@^5.66.5`（由使用方安装）。
- **非 React 前端完全不需要这个包**：直接用 `@floatctf/sdk`。

---

## 6. 认证（Authentication）

**Token 存储属于前端。** SDK 只问「当前 token 是什么」，以及「401 之后你想做什么」。

```ts
// src/auth/session.ts —— 存储策略由前端自由选择
// （模块作用域变量 / localStorage / 状态库 / Context，都是合法的）
export let userToken: string | null = null;
export let adminToken: string | null = null;

export function setUserToken(token: string | null): void {
	userToken = token;
}
export function setAdminToken(token: string | null): void {
	adminToken = token;
}
```

```ts
// src/mount.ts
import { createFloatCTFClient } from "@floatctf/sdk";
import type { FloatCTFMountContext } from "@floatctf/frontend-runtime";
import { adminToken, setAdminToken, setUserToken, userToken } from "./auth/session";

export function mount(context: FloatCTFMountContext): () => void {
	const client = createFloatCTFClient({
		baseUrl: context.apiBaseUrl, // 生产同源 → "/api"
		// adminBaseUrl 默认 `${baseUrl}/admin`，需要时可显式覆盖
		getUserToken: () => userToken,
		getAdminToken: () => adminToken,
		onUnauthorized: ({ scope }) => {
			// 401 之后做什么完全由前端决定：这里只清 token 并交给你的路由守卫
			if (scope === "admin") setAdminToken(null);
			else setUserToken(null);
		},
	});

	// …启动你自己的框架 / 路由 / 状态，渲染进 context.root…

	return () => {
		// 卸载：销毁你的应用
	};
}
```

必须遵守：

- **选手端与管理端是两个独立作用域**：`getUserToken` / `getAdminToken` 各自取值；
  `onUnauthorized` 的 `scope` 是 `"user" | "admin"`。
- **SDK 从不触碰 `localStorage`**，**从不导航**。持久化是前端的自主选择。
- **绝不把 token 放进 URL / query string**。REST 与 SSE 的 Bearer 都走
  `Authorization` 头（SSE 由 SDK 处理，见 §14）。
- 登出 UX 与路由守卫归前端所有。
- 登录调用（真实公共面）：
  `client.service.users.login({ username, password })` → `UniResponse<string>`；
  `client.admin.login({ username, password })` → `UniResponse<string>`。

**仅作示例参照**（不要 import）：Default 的接线在
`frontends/default/src/api/client.ts`（`onUnauthorized` 清 token + 跳转）与
`frontends/default/src/stores/AuthStore.ts`（Zustand `persist`）。外部前端必须写成
自己的形式。

---

## 7. `mount(context)` 生命周期

制品入口必须导出 `mount`（命名导出或 default 导出，两者都被
`normalizeFrontendModule` 容忍）：

```ts
import type {
	FloatCTFFrontendModule,
	FloatCTFMountContext,
} from "@floatctf/frontend-runtime";

export const manifest = { id: "my-frontend", version: "1.0.0" };

export function mount(context: FloatCTFMountContext): () => void {
	const host = document.createElement("div");
	context.root.replaceChildren(host);

	// 你的框架在这里挂载：app.mount(host) / root.render(...) / new App(host)

	return () => {
		// 清理：销毁应用、取消订阅、abort SSE
		host.remove();
	};
}

const frontendModule: FloatCTFFrontendModule = { manifest, mount };
export default frontendModule;
```

- 允许 `mount` 是 `async`；**抛错会触发 bootstrap 回退**。
- 返回值可为 `void`、清理函数、或 `Promise<清理函数 | undefined>`。
- `manifest` 可选，**仅用于诊断**；平台以本地注册表为准。

### 7.1 `FloatCTFMountContext` 的每一个字段

| 字段 | 类型 | 真实取值 / 含义 |
|---|---|---|
| `root` | `HTMLElement` | 渲染宿主；bootstrap **保证已清空** |
| `apiBaseUrl` | `string` | API base URL，默认 `/api`（`DEFAULT_API_BASE_URL`），生产同源 |
| `assetBaseUrl` | `string` | 本制品根目录的同源绝对路径，形如 `/__floatctf/frontends/<id>/<version>` |
| `frontendId` | `string` | 当前前端 ID（诊断） |
| `frontendVersion` | `string` | 当前前端版本（诊断） |
| `platformVersion` | `string` | 平台版本（`/api/frontend` 返回，纯展示/诊断） |
| `apiContractVersion` | `string` | 对外 HTTP API 契约 major，当前 `"1"` |
| `frontendRuntimeVersion` | `string` | 前端运行时契约 major，当前 `"1"` |
| `capabilities` | `readonly string[]` | 平台真实且稳定的能力标记 |

`capabilities` 白名单（后端 `apps/api/src/core/contract.rs` 的
`PLATFORM_CAPABILITIES`）：`jeopardy`、`awd`、`awdp`、`discussions`、`writeups`、
`web_terminal`。用 `new Set(context.capabilities).has("awd")` 探测，**不要**把不在白名单
里的字符串当作平台保证。

### 7.2 `context.assetBaseUrl` 是强制的

生产制品路径**带版本号**（`/__floatctf/frontends/<id>/<version>/...`），且带
`immutable` 长缓存。因此：

- **MUST** 用 `context.assetBaseUrl` 加载本前端自己的图片 / 字体 / 静态资产；
- **MUST NOT** 假设前端位于站点根 `/`，也**不得**硬编码 `/logo.svg` 之类的绝对路径；
- 构建器的 `base` 必须设为相对路径（例如 Vite `base: "./"`），否则 chunk 会指向错误的源。

```ts
export function assetUrl(context: FloatCTFMountContext, path: string): string {
	return `${context.assetBaseUrl.replace(/\/+$/, "")}/${path.replace(/^\/+/, "")}`;
}
```

---

## 8. 路由自由（Routing freedom）

浏览器路由**完全归前端所有**。平台不要求 Default 的路径、层级或导航结构。

可选方案（任选，非穷举）：TanStack Router、React Router、Vue Router、SvelteKit 路由、
Solid Router、自研路由、直接使用 History API、hash routing。

平台侧的保证（见 [ARTIFACT.md §4](./ARTIFACT.md) 的 Caddy 映射）：

- `/api/**` 先匹配，**不会**被前端资产遮蔽；
- `/__floatctf/frontends/<id>/<version>/**` 直接提供版本化资产；
- **其它任意路径**回退到 bootstrap `index.html`（SPA fallback），因此前端自己的深链
  刷新可用；
- bootstrap 本身完全不解析你的路由。

由此产生的硬性要求：

- **MUST** 测试「直接刷新自己的深层路由」这一条路径（例如把你的
  `/arena/<id>/live` 直接粘进地址栏并刷新）。这是 SPA fallback 是否真正生效的
  **唯一**验证方式，`vite build` 成功**不**证明它。
- **MUST NOT** 复制 Default 的路由树、页面层级或 URL 命名；那是 Default 的实现细节。

---

## 9. 如何正确地研读 Default Frontend

此节的区分是**关键**：读 Default 是为了**行为**，不是为了**外观**。

### 9.1 六步方法论

1. **定位 Default 的路由/页面**：在 `frontends/default/src/routes/**` 找到覆盖该能力的
   页面文件，读它的 `createFileRoute("...")` 与数据 hooks。
2. **看它调用哪个 SDK API**：记录域名面与方法（例如 `serviceApi.awd.scores(id)`），
   这是**公共契约**，你可以直接调用。
3. **看它的状态 / 权限 / 空 / 错误 / 实时行为**：loading 时渲染什么？空数据渲染什么？
   401 走哪里？是否轮询或订阅 SSE？权限不足时隐藏还是禁用？
4. **必要时看后端公开 DTO**：DTO 类型从 `@floatctf/sdk` 导入（`AwdScoreRow` 等），
   字段含义以 SDK 类型 + `docs/frontend/CAPABILITY-MATRIX.md` 为准。
5. **在你自己的 UX 里重新实现 BEHAVIOR**：用你自己的框架、组件、布局、路由。
6. **不要因为 Default 存在某种布局就照抄它**。配色、字体、栅格、导航、页面层级全部
   由你决定（§12）。

### 9.2 三个落地示例

**A. AWD 积分榜**

- Default 参照：`frontends/default/src/routes/service/events/awd.$id/scoreboard.tsx`。
- 公共 API：`client.service.events.get(id)`（取赛事信息/我的队伍）+
  `client.service.awd.scores(id)` → `UniResponse<AwdScoreRow[]>`。
- Default 行为：loading → spinner；错误 → 显示 message；约 30s `refetchInterval`；按队伍
  渲染分数行并高亮自己所在队伍。
- 你要重新实现的**行为**：真实积分行、我的队伍高亮、加载/错误/空态、以及（可选）订阅
  `/events/<id>/awd/stream` 做低延迟刷新。**不要**照抄 DataTable/Primer 的外观。

**B. 管理端题目管理**

- Default 参照：`frontends/default/src/routes/admin/challenges.tsx`。
- 公共 API：`client.admin.challenges.fetch`（列表）、`client.admin.challenges.create`、
  `client.admin.challenges.buildChallenges(ids)`。
- 你要重新实现的行为：真实列表 + 新建 + 批量构建 + 管理员鉴权 + 错误展示。
- `GenericTable` 的增删改查形态是 **Default 的视觉约定**，不是平台契约。

**C. 选手登录**

- Default 参照：`frontends/default/src/routes/index.tsx`（`createFileRoute("/")`，
  并带 `ServiceRouteGuardWithRedirect`）；管理端登录在
  `frontends/default/src/routes/admin/index.tsx`（`createFileRoute("/admin/")`）。
- 公共 API：`client.service.users.login({ username, password })` →
  `UniResponse<string>`（token）；管理端 `client.admin.login(...)`。
- 你要重新实现的行为：两个独立 token 作用域、失败时的错误提示、401 清 token、
  登录成功后进入你自己的首页。Default 的 Primer 表单与跳转目标（`/service`）**不是**
  契约。

---

## 10. 完整性口径（Completeness profiles）

「完整」指的是**能力覆盖**，不是 URL / 页面数量对齐。同一个能力，Default 用多个页面
表达，你的前端可以合并成一页；反之亦然。判定基准是
[CAPABILITY-MATRIX.md](./CAPABILITY-MATRIX.md) 中的能力行。

### 10.1 Complete Frontend

- **默认解读**：用户说「创建一个 FloatCTF Frontend」且未限定范围时，就是这个。
- **MUST** 覆盖 [CAPABILITY-MATRIX.md](./CAPABILITY-MATRIX.md) 中标为 `required`
  的全部能力。
- **MUST NOT** 以「路由数量不同」「页面数量更少」「信息架构不同」为由判定不完整。
- 完整性 = 能力覆盖；URL parity 不是要求。

### 10.2 Player Frontend

- 只面向选手：登录、赛事浏览/加入、Jeopardy 解题、AWD/AWDP 选手侧、榜单、讨论、
  writeup 等**选手有权访问**的能力子集。
- **MUST NOT** 实现管理端能力（也不该持有 admin token）。
- **MUST** 在 `FRONTEND-PLAN.md` 中显式写为 partial 并列出未覆盖的能力。

### 10.3 Admin Frontend

- 只面向管理员：SuperAdmin/管理员登录、赛事与题目配置、用户/队伍、AWD/AWDP 运维、
  日志与设置等。
- **MUST NOT** 假装覆盖选手端能力。
- **MUST** 在计划中显式写为 partial。

### 10.4 Specialized Frontend

- 大屏积分榜展示、kiosk、特定赛事客户端等**刻意收窄**的前端。
- **MUST** 被明确标注为 **incomplete**（在 `FRONTEND-PLAN.md`、README 与交付说明中都写）。
- **MUST NOT** 用「演示落地页 + 假数据」冒充完整前端。只显示一个静态 landing page
  **不是** complete frontend，也不算任何 profile 的完成。

---

## 11. 编码前必须先产出计划

在写实现代码之前，**MUST** 先在外部仓库写出 `FRONTEND-PLAN.md`。模板字段如下：

```markdown
# FRONTEND-PLAN — <FRONTEND_NAME>

## 1. Identity
- Frontend id: <FRONTEND_ID>          # [a-z0-9][a-z0-9._-]*，≤64，见 ARTIFACT.md
- Name / version: <FRONTEND_NAME> / <x.y.z>
- Scope: complete | player-only | admin-only | scoreboard-only | kiosk

## 2. Technology
- Framework: <FRAMEWORK>
- Router: <...>
- State + auth storage: <...>          # token 存哪里、怎么清
- Styling: <...>
- Data fetching: <...>

## 3. Visual direction
- Design language: <...>
- Typography / spacing / responsive: <...>
- Motion: <...>
- Accessibility: <...>

## 4. Information architecture
- Sections: <...>
- Route plan: <path → capability>      # 与 Default 无关，自定

## 5. Capability coverage
| Capability (CAPABILITY-MATRIX) | required? | covered? | route |
|---|---|---|---|
# 结论：complete 或 partial（partial 必须列出缺口）

## 6. Auth
- User token strategy: <...>
- Admin token strategy: <...>
- Unauthorized behaviour: <...>

## 7. Realtime
- AWD: <stream / polling / none>
- AWDP: <stream / polling / none>

## 8. Artifact
- Entry: <...>
- Styles: <...>
- Build script: <...>
- outputDir: <...>
```

说明：本文**不**要求每个琐碎决定都等用户批准——除非用户明确要求交互式设计评审。
但 `FRONTEND-PLAN.md` 必须在写实现前存在，并且是后续自查覆盖率的依据。

---

## 12. 视觉自由（Visual freedom）

- 除用户明确要求，新前端 **SHOULD NOT** 长得像 Default。
- 允许的方向（非穷举）：Material、cyberpunk、terminal-inspired、glassmorphism、
  minimalist、dashboard-heavy、mobile-first、editorial、自定义品牌系统。
- **但是**：视觉创意**绝不能**改变后端语义。

### 12.1 数据真实性（硬规则）

- **MUST** 使用真实 API 数据渲染。
- **MUST NOT** 用假数据 / mock / 占位数据充当生产平台数据；**禁止**用
  「示例比分」「示例题目」掩盖未完成的集成。
- mock 只允许存在于**隔离的**测试 / Storybook / dev fixture 中，**绝不**作为 live 平台
  数据随制品发布。
- 状态判定（进行中 / 已结束 / 暂停 / 封禁 …）**MUST** 与后端一致，不得自造。

---

## 13. 错误 / 空 / 加载状态

完整性**包含**这些状态。缺任意一项即视为未完成：

- loading（首次加载 + 后台刷新）
- empty（查询成功但无数据）
- permission-denied（有 token 但无权限）
- network error（断网 / 超时 / CORS）
- backend validation error（平台业务码非 0）
- expired auth（401 → 清 token → 你的登录流程）
- not-found（路由与实体两层）
- realtime disconnected / reconnecting

**MUST NOT** 在错误时渲染空白屏。错误必须可见、可读、可恢复。

使用 `FloatCTFError`（`packages/sdk/src/errors.ts`），不要假设 Axios 内部结构：

| 字段 | 含义 |
|---|---|
| `name` | 恒为 `"FloatCTFError"` |
| `kind` | `"http" \| "platform" \| "network" \| "unknown"` |
| `httpStatus` | HTTP 状态码（无响应时 `undefined`） |
| `code` | 平台业务码（`UniResponse.code`） |
| `platformMessage` | 平台文案（`UniResponse.message`） |
| `original` | 原始错误（通常是 Axios 错误），深度诊断用 |
| `unauthorized` | 是否 401 |
| `response` | Axios 兼容形态 `{ status, statusText, data, headers }` |
| `displayMessage` | 最适合展示给用户的文案（平台文案优先） |
| `toJSON()` | 序列化 |

```ts
import { FloatCTFError } from "@floatctf/sdk";

try {
	await client.service.events.fetch({ limit: 20 });
} catch (error) {
	if (error instanceof FloatCTFError) {
		const { kind, httpStatus, code, unauthorized, displayMessage } = error;
		// 按 kind 分支：network → 重试入口；platform/http → displayMessage；
		// unauthorized → 走你自己的登录流程
	}
}
```

---

## 14. SSE / 实时

实时通道由客户端发起，SDK 负责传输（fetch + `ReadableStream`，Bearer 走
`Authorization` 头，**永不**进 query string）。

- 选手端：`client.sse.connect(options)` —— 使用**选手端** base URL + `getUserToken`；
- 管理端：`client.sse.connectAdmin(options)` —— 使用**管理端** base URL + `getAdminToken`；
- 相对 URL 由 `resolveSseUrl` 解析到**该客户端自己的** base URL 上（跨源/自建 base URL
  的前端不会出现「REST 正确、实时打错源」）。

`FloatCTFSseOptions` = `{ url: string; getToken?: () => string | null }` **加上**
`Omit<ConnectSseOptions, "url" | "getToken">`。因此 `headers` 与 `signal` **仍然是必填
字段**——传 `headers: {}` 和一个 `AbortController.signal` 即可。

```ts
import type { FloatCTFClient, SseEvent } from "@floatctf/sdk";

export function watchAwd(client: FloatCTFClient, eventId: string): () => void {
	const controller = new AbortController();

	const connection = client.sse.connect({
		url: `/events/${eventId}/awd/stream`, // 相对 → client.baseUrl
		headers: {},
		signal: controller.signal,
		onEvent: (event: SseEvent) => {
			// event.event / event.data / event.id / event.retry
		},
		onStateChange: (status) => {
			// status.state: idle|connecting|connected|reconnecting|auth_error|error|closed
		},
	});

	return () => {
		controller.abort();
		connection.close();
	};
}
```

真实 SSE 端点（后端路由，已核对）：

| 通道 | 路径（相对 `client.baseUrl`） | 发起方 |
|---|---|---|
| AWD 选手 | `/events/<id>/awd/stream` | `connect` |
| AWD 管理 | `/events/<id>/awd/stream` | `connectAdmin`（admin base URL） |
| AWDP 赛事选手 | `/events/<id>/awdp/stream` | `connect` |
| AWDP run（练习/训练） | `/service/awdp/runs/<id>/stream` | `connect` |

（后端路由：`apps/api/src/modules/event/awd/api/player.rs`、`.../awd/api/admin.rs`、
`.../awdp/api/player.rs`、`.../awdp/api/training.rs`；与 [ARCHITECTURE.md §4.3](./ARCHITECTURE.md) 一致。）

重连语义由 SDK 提供：指数退避 + 抖动；401/403 停止重连；429 遵守 `Retry-After`；
支持 `Last-Event-ID`。

### 14.1 React

```ts
// client / userToken / adminToken 的装配见 §6
import { createFloatCTFReact } from "@floatctf/react";

const bindings = createFloatCTFReact({
	client,
	useUserToken: () => userToken,
	useAdminToken: () => adminToken, // 省略时复用 useUserToken
});
```

返回：`useAwdEventStream`（`{ eventId, pollMs?, preferStream?, enabled? }`）、
`useAdminAwdEventStream`、`useAwdpEventStream`、`useAwdpRunStream`（`{ runId, ... }`），
以及 `eventInfoQueryOptions`、`challengeQueryOptions`、`challengeInstanceQueryOptions`、
`systemInformationQueryOptions`、`invalidateAwdQueries`、`AWD_PLAYER_QUERY_KEYS`、
`AWD_ADMIN_QUERY_KEYS`、`client`。hook 不接受 URL 参数，URL 从传入的 `client` 派生。

### 14.2 非 React

`client.sse.*` 已经足够；`createSseParser()` / `SseEvent` / `SseParser` 也在
`@floatctf/sdk` 公开面里，需要手写解析时可用。Default 只作为**行为**参照。

---

## 15. 框架起步配方

以下均**非穷举**。React **不是**默认偏好。

| 框架 | 需要的 FloatCTF 包 | `mount()` 位置 | 路由归属 | 认证归属 | `@floatctf/react` |
|---|---|---|---|---|---|
| React | `sdk` + `frontend-runtime`（+ 可选 `react`） | 制品入口模块 | 前端 | 前端 | ✅ 适用 |
| Vue | `sdk` + `frontend-runtime` | 制品入口模块 | 前端 | 前端 | ❌ 不适用 |
| Svelte | `sdk` + `frontend-runtime` | 制品入口模块 | 前端 | 前端 | ❌ 不适用 |
| Solid | `sdk` + `frontend-runtime` | 制品入口模块 | 前端 | 前端 | ❌ 不适用 |
| 原生 TS | `sdk` + `frontend-runtime` | 制品入口模块 | 前端 | 前端 | ❌ 不适用 |

- **React**：`sdk` + `frontend-runtime`，需要 headless 数据/实时绑定时再加
  `@floatctf/react`；peer deps `react@^19`、`@tanstack/react-query@^5`。
- **Vue / Svelte / Solid**：只装 `sdk` + `frontend-runtime`；自己的框架、router、状态库
  与 UI 库任意。**不要**引入 `@floatctf/react`。
- **原生 TS**：只用 `sdk` + `frontend-runtime`；路由可用 History API / hash routing。
  这是验证「平台不要求任何框架」的最直接方式。

各框架的共同点：入口模块导出 `mount(context)`；浏览器路由、认证与 UI 都归该前端。
外部仓库流程见 [DEVELOPING.md §2](./DEVELOPING.md)。

---

## 16. 推荐的外部仓库结构

**推荐**（recommendation），**不是**运行时契约——平台只要求 `frontend.json` 的
`entry` 可被动态 import 且导出 `mount`。

```
my-floatctf-frontend/
├── AGENTS.md                 # 复制 EXTERNAL-AGENTS-TEMPLATE.md
├── README.md
├── FRONTEND-PLAN.md          # §11
├── package.json
├── floatctf.frontend.json    # 源码/构建 manifest（§17）
├── vite.config.ts            # 或你框架的等价构建配置
├── src/
│   ├── entry.ts              # 导出 mount(context)；制品入口
│   ├── app/                  # 应用装配、provider、生命周期
│   ├── api/                  # SDK 客户端接线（createFloatCTFClient）
│   ├── auth/                 # token 存储、登录流程、路由守卫
│   ├── routes/               # 你自己的路由表
│   ├── features/             # 按能力/领域切分的功能模块
│   └── styles/               # 设计系统 / 主题 / 全局样式
└── tests/
```

---

## 17. 源码 manifest / 制品

存在**两个** manifest，不要混淆（详见 [ARTIFACT.md §1](./ARTIFACT.md)）：

| 文件 | 用途 | 谁读 |
|---|---|---|
| `floatctf.frontend.json` | **源码仓库** manifest：标识前端、声明兼容性、定义构建期望 | `scripts/frontend.sh`（构建前） |
| `frontend.json` | **构建产物** manifest：运行时契约（入口、样式、兼容性） | 注册表生成、浏览器解析、发布验证 |

源码 manifest 最小形态（字段语义与校验规则以 [ARTIFACT.md](./ARTIFACT.md) 为准）：

```jsonc
{
  "schemaVersion": 1,
  "id": "my-frontend",
  "name": "My FloatCTF Frontend",
  "version": "0.1.0",
  "compatibility": { "frontendRuntime": "1", "apiContract": "1" },
  "entry": "assets/frontend.js",
  "styles": ["assets/frontend.css"],
  "build": { "packageManager": "auto", "script": "build", "outputDir": "dist" }
}
```

- `build` 段**只允许**这三个键（`packageManager` / `script` / `outputDir`），语义与
  默认值见 [ARTIFACT.md §6](./ARTIFACT.md) 与 `scripts/frontend.sh --help`；
  `script` 是 `package.json` 里的**脚本名**，绝不是 shell 片段。
- 制品 manifest **不得**包含 `build` 段（严格校验会拒绝未知字段）。
- 若构建没有产出 `frontend.json`，管理器会从源码 manifest 生成一个只含运行时字段的
  版本，并按制品契约复核后才安装（[DEVELOPING.md §4](./DEVELOPING.md)）。
- 你的构建脚本 **SHOULD** 调用 `parseFrontendManifest` 自检生成的 `frontend.json`，
  让「能构建但装不上」在构建期就失败。

### 17.1 版本不可变

前端资产 URL 带 `Cache-Control: immutable`。因此 **前端 ID + 版本 = 一组不可变字节**：

- 同 ID + 同版本 + 同内容 → 幂等成功；
- 同 ID + 同版本 + **不同内容 → 硬失败**，没有任何例外；
- **ANY released content change requires a version bump**（任何已发布内容变更都必须
  升版本号）。「改一点点再装同版本」在浏览器侧不可见，只会得到缓存不一致。
- 回滚 = 把 `currentVersion` 指针移回旧版本（旧版本目录仍在）。

---

## 18. 本地包消费（v1.0 现实）

`@floatctf/*` 在 v1.0 **没有发布到 npm registry**（npm 发布是独立可选的发布步骤，尚未
执行）。外部消费走 release tarball（三个 README 的实际口径一致）：

```bash
# 1) 在 FloatCTF 仓库里构建 release tarball（内部会先 pnpm run build:packages）
scripts/package-sdk-dist.sh /tmp/floatctf-dist

# 2) 校验（SDK-SHA256SUMS 列出 `<sha256>  <name>`）
( cd /tmp/floatctf-dist && sha256sum -c SDK-SHA256SUMS )

# 3) 在你的前端仓库（仓库之外）安装
npm install /tmp/floatctf-dist/floatctf-sdk-1.0.0.tgz \
            /tmp/floatctf-dist/floatctf-frontend-runtime-1.0.0.tgz
# React 前端再加：
npm install /tmp/floatctf-dist/floatctf-react-1.0.0.tgz
npm install react@^19 react-dom@^19 @tanstack/react-query@^5
```

- 把 `1.0.0` 换成实际 release 版本（`floatctf-sdk-<V>.tgz` 中的 `<V>`）。
- tarball 只含 `dist/`（带 `.d.ts`）、`README.md`、AGPL-3.0-only `LICENSE`；
  不含源码、测试或 `node_modules`。
- `@floatctf/sdk` 的唯一运行时依赖 `axios` 从公共 registry 安装，因此 tarball 安装
  需要网络。
- **`npm` 可靠地消费这组互相依赖的、未发布的 `@floatctf/*` tarball**——这是已测试的
  release 流程（`scripts/test-sdk-dist.sh`）。
- **已知限制**：在这些名字尚未进入 registry 时，`pnpm add` 安装这组互相依赖的 tarball
  会失败（`ERR_PNPM_FETCH_404`：pnpm 会通过 registry 解析 `@floatctf/react` 对
  `@floatctf/sdk` 的依赖，即使同一命令里传入了 SDK tarball 也一样）。因此
  **v1.0 的 tarball 流程请用 `npm install`**。
- **不要**在外部项目里使用 `workspace:*` 或指向 FloatCTF 仓库的 symlink——那是仓库内
  协议，不是对外消费方式。
- 一旦 `@floatctf/*` 真正发布到 registry，本文档的这一节可以简单更新为
  `pnpm add @floatctf/sdk`。

---

## 19. 针对 FloatCTF 的开发联调

- **生产**：同源。前端从 `context.apiBaseUrl` 拿 API 地址（默认 `/api`）。
- **独立 dev server（跨源）**：允许，但必须通过 FloatCTF 的 TOML 配置 CORS：
  在 `apps/api/config/development.toml` 增加

  ```toml
  [cors]
  allowed_origins = ["http://localhost:14000", "http://127.0.0.1:14000"]
  ```

  （结构见 `apps/api/src/core/config.rs` 的 `CorsConfig.allowed_origins`；
  缺省为 `http://localhost:3000` 与 `http://127.0.0.1`。）
- **MUST NOT** 为了 CORS 新增后端环境变量——平台配置只从 TOML / settings 读取，这是
  铁律。
- 外部前端 **MAY** 用**自己的构建期变量**指向 API（例如 `VITE_API_BASE_URL=http://127.0.0.1:7780/api`）。
  这是**前端自己的**构建配置，不是平台契约：平台配置与前端构建配置必须保持区分。
- 用未发布的本地包时，按 §18 打包后用 `npm install file:...` 引用。

---

## 20. 安装验收流程

**MUST NOT** 仅仅因为 `vite build`（或等价命令）成功就宣称前端完成。必须走完以下流程。

1. 构建：在外部仓库跑 `build` 段声明的脚本。
2. 确认产物里存在 `frontend.json`（制品 manifest，无 `build` 段）。
3. 校验 manifest：`scripts/frontend.sh verify <artifact.tar.gz>`（或打归档后校验）。
4. 安装：`sudo $FLOATCTF_HOME/frontend.sh install <源码目录 | Git URL | .tar.gz>`。
5. `frontend.sh list` 确认已安装；`frontend.sh info <id>` 确认版本与兼容性。
6. 激活：管理端 → 设置 → 前端 → 选择 → 保存并切换（写 `FRONTEND_ACTIVE`）。
   CLI **不会**替你激活。
7. 浏览器刷新页面，确认加载的是你的前端（而不是回退到 default）。
8. 验证**深层路由直接刷新**（地址栏直接输入 + F5）——SPA fallback 生效。
9. 验证选手鉴权：登录、登出、401 后行为、token 不被放进 URL。
10. 若覆盖管理端：验证管理员鉴权与 `scope === "admin"` 的 401 行为。
11. 验证**真实 API 数据**：页面上每条数据都来自后端，没有假数据/占位。
12. 若覆盖实时：验证 SSE 连接、断线重连、`auth_error` 行为；并确认未用 query string
    传 token。
13. 验证破窗回退：任意页面加 `?frontend=default` 应回到内置前端，且**不**修改设置、
    不需登录。

真实子命令以 `scripts/frontend.sh --help` 为准：
`help | list | info <id> [version] | verify <artifact.tar.gz> | install <source> [--ref|--node-image|--no-build|--make-current|--platform|--dry-run] | remove <id> [version] | set-current <id> <version>`。
完整流程与 FAQ 见 [DEVELOPING.md](./DEVELOPING.md)。

---

## 21. Prompt 模板：创建一个 Frontend

把下面整段（替换 `<...>`）交给编码代理：

```text
你要为一个 FloatCTF 实例创建一个**全新的可插拔前端**（不是修改官方 Default 前端）。

参数：
- <FRONTEND_NAME>: 前端显示名
- <FRONTEND_ID>:   前端 ID（[a-z0-9][a-z0-9._-]*，≤64）
- <FRAMEWORK>:     任意浏览器框架/技术栈（React / Vue / Svelte / Solid / 原生 TS / …）
- <VISUAL_DIRECTION>: 视觉方向（例如 cyberpunk / minimal / material / terminal-inspired）
- <SCOPE>:         complete | player-only | admin-only | scoreboard-only | kiosk

必读（按顺序）：
1. docs/frontend/AI-FRONTEND-GUIDE.md          —— 本任务的权威手册
2. docs/frontend/CAPABILITY-MATRIX.md          —— 能力覆盖基准（required 行）
3. docs/frontend/ARTIFACT.md                   —— manifest / 注册表 / 版本不可变
4. docs/frontend/ARCHITECTURE.md               —— mount 契约 / 依赖方向 / 信任模型
5. packages/sdk/README.md、packages/react/README.md、
   packages/frontend-runtime/README.md        —— 公共包的真实消费方式

硬性规则：
- 只用公共包：@floatctf/sdk + @floatctf/frontend-runtime（仅 React 可加 @floatctf/react）。
  禁止 import apps/web/*、frontends/default/*、packages/*/src/*、@/...、任何逃逸进
  FloatCTF monorepo 的相对路径或私有 alias。
- frontends/default 只作**语义/行为参照**（业务能力、API 调用方式、鉴权语义、状态语义、
  错误与边界、实时行为、用户/管理员可见信息）。它不是视觉模板，更不是依赖。
  配色、字体、组件、布局、页面层级、导航、路由路径全部由本前端自主决定。
- 不得修改 frontends/default，不得为了前端方便而擅自改后端。若发现真实的公共契约缺口，
  停下来，输出 `PUBLIC SDK GAP: <细节>` 并询问，不要自行改后端。
- Token 归本前端所有：SDK 不写 localStorage、不跳转；绝不把 token 放进 URL。
- 禁止假数据/占位数据冒充真实平台数据；mock 仅限隔离测试。
- 视觉自由不等于语义自由：真实 API 数据、与后端一致的状态判定。

流程：
1. 先写 FRONTEND-PLAN.md（identity / technology / visual direction / IA+路由 /
   capability coverage 表 / auth / realtime / artifact 八段）。
2. 按 CAPABILITY-MATRIX 的 required 能力逐项实现（<SCOPE> 若为部分范围，在计划与
   README 中显式标注 partial 并列出缺口）。
3. 实现 mount(context)：使用 context.apiBaseUrl 建客户端，使用 context.assetBaseUrl
   加载自己的资产，返回清理函数。
4. 覆盖 loading / empty / permission-denied / network error / validation error /
   expired auth / not-found / realtime 断线状态。
5. 构建产出 frontend.json，并用 parseFrontendManifest 自检。
6. 走完 AI-FRONTEND-GUIDE.md §20 的 13 步验收流程（含深链刷新、破窗 ?frontend=default、
   真实数据校验）。
7. 汇报：覆盖了哪些能力、哪些没有、以及验收每一步的真实结果。
```

---

## 附：禁项速查

- ❌ 修改 `frontends/default/` 来「顺便」实现新前端
- ❌ import `frontends/default/*`、`apps/web/*`、`packages/*/src/*`、`@/...`
- ❌ 把 token 放进 URL / query string
- ❌ 硬编码绝对资产路径，忽略 `context.assetBaseUrl`
- ❌ 假设站点根 `/` 就是前端根
- ❌ 用假数据 / 占位数据冒充 live 数据
- ❌ 照抄 Default 的路由树 / 配色 / 导航 / 组件
- ❌ 同 ID 同版本改内容（必须升版本号）
- ❌ 把 `@floatctf/react` 当成平台默认或非 React 前端的依赖
- ❌ `vite build` 通过就宣布完成
