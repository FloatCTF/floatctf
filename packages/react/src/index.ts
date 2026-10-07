/**
 * `@floatctf/react` —— FloatCTF 的**可选** React headless 绑定。
 *
 * 边界（必须严格遵守）：
 * - ✅ 允许：query option 工厂、query key 常量、可复用 mutation/hook、
 *   SSE → React Query 失效逻辑、数据/状态/动作 hook
 * - ❌ 禁止：Header / Sidebar / Table 视觉组件、Primer、Tailwind、CSS、
 *   路由、页面、导航、对话框、任何 UX 流程
 *
 * Hook 只暴露 data / actions / state，**不返回 JSX**。
 *
 * 非 React 前端**完全不需要**这个包：它们直接用 `@floatctf/sdk`。
 *
 * ```ts
 * const { useAwdEventStream, eventInfoQueryOptions } = createFloatCTFReact({
 *   client,
 *   useUserToken: () => useAuthStore((s) => s.token),
 *   useAdminToken: () => useAuthStore((s) => s.adminToken),
 * });
 * ```
 */

import type { FloatCTFClient } from "@floatctf/sdk";

import {
	AWD_ADMIN_QUERY_KEYS,
	AWD_PLAYER_QUERY_KEYS,
	invalidateAwdQueries,
} from "./awdInvalidation.js";
import { createQueryFactories } from "./queries/index.js";
import type { UseTokenSource } from "./tokenSource.js";
import { createUseAdminAwdEventStream } from "./useAdminAwdEventStream.js";
import { createUseAwdEventStream } from "./useAwdEventStream.js";
import { createUseAwdpEventStream } from "./useAwdpEventStream.js";
import { createUseAwdpRunStream } from "./useAwdpRunStream.js";

export interface CreateFloatCTFReactOptions {
	/** 已绑定的 FloatCTF 客户端（来自 `@floatctf/sdk` 的 `createFloatCTFClient()`）。 */
	client: FloatCTFClient;
	/** 前端自有的"读当前选手 token"hook。 */
	useUserToken: UseTokenSource;
	/** 前端自有的"读当前管理员 token"hook；省略时复用 `useUserToken`。 */
	useAdminToken?: UseTokenSource;
}

export function createFloatCTFReact(options: CreateFloatCTFReactOptions) {
	const { client } = options;
	const useUserToken = options.useUserToken;
	const useAdminToken = options.useAdminToken ?? options.useUserToken;

	return {
		client,
		// ── 实时事件流（SSE + 轮询兜底 + React Query 失效）──
		useAwdEventStream: createUseAwdEventStream(client, useUserToken),
		useAdminAwdEventStream: createUseAdminAwdEventStream(client, useAdminToken),
		useAwdpEventStream: createUseAwdpEventStream(client, useUserToken),
		useAwdpRunStream: createUseAwdpRunStream(client, useUserToken),
		// ── query options 工厂 ──
		...createQueryFactories(client),
		// ── 失效工具（页面手动刷新与事件流共用同一份 key 常量）──
		invalidateAwdQueries,
		AWD_PLAYER_QUERY_KEYS,
		AWD_ADMIN_QUERY_KEYS,
	};
}

export type FloatCTFReactBindings = ReturnType<typeof createFloatCTFReact>;

export {
	AWD_ADMIN_QUERY_KEYS,
	AWD_PLAYER_QUERY_KEYS,
	invalidateAwdQueries,
} from "./awdInvalidation.js";
export type { UseTokenSource } from "./tokenSource.js";
export type { AwdpStreamEvent, UseAwdpEventStreamOptions } from "./useAwdpEventStream.js";
export type { UseAwdpRunStreamOptions } from "./useAwdpRunStream.js";
export type {
	AwdStreamEvent,
	UseAwdEventStreamOptions,
} from "./useAwdEventStream.js";
export type { UseAdminAwdEventStreamOptions } from "./useAdminAwdEventStream.js";
