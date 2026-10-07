/**
 * `createFloatCTFClient()` —— SDK 的唯一入口工厂。
 *
 * 它做三件事：
 * 1. 建立并**绑定**共享 HTTP 传输（baseURL + Bearer 注入 + 错误归一化 + 401 回调）
 * 2. 返回按领域分组的 API 门面（`service` / `admin` / `awd` / `awdp` / `awdpRuns`）
 * 3. 暴露实时（SSE）工具与可以 `dispose()` 的生命周期
 *
 * 前端负责注入"token 从哪来"和"401 干什么"；SDK 负责其余全部传输机制。
 *
 * ```ts
 * const client = createFloatCTFClient({
 *   baseUrl: "/api",
 *   getUserToken: () => useAuthStore.getState().token,
 *   getAdminToken: () => useAuthStore.getState().adminToken,
 *   onUnauthorized: ({ scope }) => {
 *     if (scope === "admin") { useAuthStore.getState().removeAdminToken(); router.navigate({ to: "/admin" }); }
 *     else { useAuthStore.getState().removeToken(); router.navigate({ to: "/" }); }
 *   },
 * });
 * ```
 */

import type { AxiosInstance } from "axios";

import {
	type FloatCTFClientOptions,
	admin_api,
	service_api,
	bindHttpClients,
} from "./transport.js";

import { adminApi, serviceApi } from "./api/index.js";
import { awdAdminApi, awdPlayerApi } from "./api/awd.js";
import { awdpAdminApi, awdpPlayerApi } from "./api/awdp.js";
import { awdpRunApi } from "./api/awdpRuns.js";
import { connectSse, type ConnectSseOptions, type SseConnection } from "./sse/connectSse.js";
import {
	createSseParser,
	type SseEvent,
	type SseParser,
} from "./sse/parser.js";

export type { FloatCTFClientOptions } from "./transport.js";

export interface FloatCTFClient {
	/** 选手端 API base URL。 */
	readonly baseUrl: string;
	/** 管理端 API base URL。 */
	readonly adminBaseUrl: string;
	/** 底层 axios 实例（逃生舱：自定义拦截器/上传进度/流式响应）。 */
	readonly transport: {
		readonly service: AxiosInstance;
		readonly admin: AxiosInstance;
	};
	/** 领域门面。 */
	readonly service: typeof serviceApi;
	readonly admin: typeof adminApi;
	readonly awd: {
		readonly player: typeof awdPlayerApi;
		readonly admin: typeof awdAdminApi;
	};
	readonly awdp: {
		readonly player: typeof awdpPlayerApi;
		readonly admin: typeof awdpAdminApi;
		readonly runs: typeof awdpRunApi;
	};
	/** 实时传输（fetch-based SSE，Bearer 走 Authorization 头）。 */
	readonly sse: {
		connect(options: Omit<ConnectSseOptions, "getToken"> & { getToken?: () => string | null }): SseConnection;
		createParser(): SseParser;
	};
	/** 解绑传输（登出 / 卸载 / 测试）。 */
	dispose(): void;
}

/**
 * 创建并绑定 FloatCTF 客户端。
 *
 * ⚠️ 同一个页面同时只支持**一个**绑定的客户端（bootstrap 每次加载只挂载一个前端）。
 * 再次创建会替换绑定并使先前客户端的传输失效——这是刻意的、有文档的约束，
 * 换来的是 API 模块不需要到处传递实例。
 */
export function createFloatCTFClient(
	options: FloatCTFClientOptions = {},
): FloatCTFClient {
	const { service, admin, dispose } = bindHttpClients(options);
	const baseUrl = (options.baseUrl ?? "/api").replace(/\/+$/, "");
	const adminBaseUrl = (options.adminBaseUrl ?? `${baseUrl}/admin`).replace(/\/+$/, "");

	return {
		baseUrl,
		adminBaseUrl,
		transport: { service, admin },
		service: serviceApi,
		admin: adminApi,
		awd: { player: awdPlayerApi, admin: awdAdminApi },
		awdp: { player: awdpPlayerApi, admin: awdpAdminApi, runs: awdpRunApi },
		sse: {
			connect: (sseOptions) =>
				connectSse({
					...sseOptions,
					// 默认复用客户端注入的 token 来源，除非调用方显式覆盖。
					getToken: sseOptions.getToken ?? (() => options.getUserToken?.() ?? null),
				}),
			createParser: createSseParser,
		},
		dispose,
	};
}

/** 当前绑定的 handle（供需要直接使用 handle 的 SDK 内部/高级场景）。 */
export const httpClients = { service: service_api, admin: admin_api } as const;

export type { SseConnection, SseEvent, SseParser, ConnectSseOptions };
