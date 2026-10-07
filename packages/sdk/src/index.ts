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
 * 内容：
 * - `createFloatCTFClient()` / `FloatCTFError` / `UniResponse`（传输与错误契约）
 * - 领域 API 客户端（选手端 / 管理端 / AWD / AWDP）
 * - fetch-based SSE（Bearer 走 Authorization 头）
 * - 前端可见的 DTO 类型
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
	admin_api,
	bindHttpClients,
	resetHttpClientsForTests,
	service_api,
	type FloatCTFAuthScope,
	type FloatCTFClientOptions,
	type FloatCTFHttpClient,
	type UnauthorizedContext,
} from "./transport.js";

// ── 客户端工厂 ──────────────────────────────────────────────────────────────
export {
	createFloatCTFClient,
	httpClients,
	type FloatCTFClient,
} from "./client.js";

// ── 视图无关 DTO 类型 ───────────────────────────────────────────────────────
export * from "./types/index.js";

// ── SSE ─────────────────────────────────────────────────────────────────────
export {
	connectSse,
	type ConnectSseOptions,
	type SseConnection,
	type SseConnectionState,
	type SseConnectionStatus,
} from "./sse/connectSse.js";
export { createSseParser, type SseEvent, type SseParser } from "./sse/parser.js";

// ── 领域 API（门面 + 各模块具名导出）────────────────────────────────────────
export * from "./api/index.js";
export * from "./api/awd.js";
export * from "./api/awdp.js";
export * from "./api/awdpRuns.js";
