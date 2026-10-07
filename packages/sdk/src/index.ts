/**
 * `@floatctf/sdk` —— FloatCTF 平台客户端 SDK（**框架无关**）。
 *
 * 依赖方向（唯一允许的方向）：
 *
 * ```
 * FloatCTF Backend ── REST / SSE / Bearer ──▶ @floatctf/sdk ──▶ @floatctf/react ──▶ Frontend
 * ```
 *
 * 本包**绝不**依赖：React、路由、Zustand、Primer、Tailwind、任何应用页面。
 * 认证 token 的来源与 401 之后的 UI 反应**全部由调用方注入**
 * （见 `createFloatCTFClient`）。
 *
 * ## 实例隔离
 *
 * SDK 里**没有**"当前客户端"这类模块级状态。`createFloatCTFClient()` 返回的对象
 * 自带传输与领域门面，可同时存在多个、互不影响：
 *
 * ```ts
 * import { createFloatCTFClient } from "@floatctf/sdk";
 *
 * const client = createFloatCTFClient({
 *   baseUrl: "/api",
 *   getUserToken: () => token,
 * });
 * await client.service.events.fetch();
 * await client.admin.settings.fetch();
 * client.sse.connect({ url: `/events/${id}/awd/stream`, ... });
 * ```
 *
 * 领域模块以**工厂**形式导出（`createEventServiceApi(http)` 等）供高级用法组合；
 * 常规用法请直接使用 `client.service.*` / `client.admin.*` / `client.awd.*` /
 * `client.awdp.*`。
 *
 * 生成的数据库实体是独立入口：`@floatctf/sdk/entity`（不要把 DB 列类型混进 API 面）。
 */

// ── 协议与错误 ──────────────────────────────────────────────────────────────
export { UNI_SUCCESS_CODE, type QueryParams, type UniResponse } from "./protocol.js";
export {
	FloatCTFError,
	floatCTFErrorFromEnvelope,
	toFloatCTFError,
	type FloatCTFErrorKind,
	type FloatCTFErrorResponse,
} from "./errors.js";

// ── 传输 ────────────────────────────────────────────────────────────────────
export {
	DEFAULT_API_BASE_URL,
	createFloatCTFTransport,
	normalizeBaseUrl,
	resolveBaseUrls,
	type FloatCTFAuthScope,
	type FloatCTFClientOptions,
	type FloatCTFHttpClient,
	type FloatCTFRequestConfig,
	type FloatCTFTransport,
	type UnauthorizedContext,
} from "./transport.js";

// ── 客户端工厂 ──────────────────────────────────────────────────────────────
export {
	createFloatCTFClient,
	resolveSseUrl,
	type FloatCTFClient,
	type FloatCTFSseOptions,
} from "./client.js";

// ── 视图无关 DTO 类型 ───────────────────────────────────────────────────────
export * from "./types/index.js";

// ── SSE（独立底层工具；领域 hook 见 @floatctf/react）─────────────────────────
export {
	connectSse,
	type ConnectSseOptions,
	type SseConnection,
	type SseConnectionState,
	type SseConnectionStatus,
} from "./sse/connectSse.js";
export { createSseParser, type SseEvent, type SseParser } from "./sse/parser.js";

// ── 领域 API：门面工厂 + 各模块工厂 + 类型 ──────────────────────────────────
export * from "./api/index.js";
export * from "./api/awd.js";
export * from "./api/awdp.js";
export * from "./api/awdpRuns.js";
