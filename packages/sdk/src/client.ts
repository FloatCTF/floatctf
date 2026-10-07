/**
 * `createFloatCTFClient()` —— SDK 的唯一入口工厂。
 *
 * 它做三件事：
 * 1. 为**这个实例**建立两个 HTTP 传输（选手端 / 管理端：baseURL + Bearer 注入 +
 *    错误归一化 + 401 回调）
 * 2. 基于这两个 handle 构建按领域分组的 API 门面
 *    （`service` / `admin` / `awd` / `awdp` / `sse`）
 * 3. 返回一个**完全独立**的客户端对象
 *
 * 前端负责注入"token 从哪来"和"401 干什么"；SDK 负责其余全部传输机制。
 *
 * ## 实例隔离
 *
 * 不存在模块级共享绑定，也没有"当前客户端"。同一进程里可以同时创建任意多个客户端，
 * 它们各自的 base URL、token 来源与错误回调互不影响，创建顺序也无关：
 *
 * ```ts
 * const a = createFloatCTFClient({ baseUrl: "https://a.example/api", getUserToken: () => "A" });
 * const b = createFloatCTFClient({ baseUrl: "https://b.example/api", getUserToken: () => "B" });
 *
 * await b.service.events.fetch(); // → b.example + "B"
 * await a.service.events.fetch(); // → a.example + "A"
 * ```
 */

import type { AxiosInstance } from "axios";

import { type AdminApi, type ServiceApi, createAdminApi, createServiceApi } from "./api/index.js";
import { type AwdAdminApi, type AwdPlayerApi, createAwdAdminApi, createAwdPlayerApi } from "./api/awd.js";
import { type AwdpAdminApi, type AwdpPlayerApi, createAwdpAdminApi, createAwdpPlayerApi } from "./api/awdp.js";
import { type AwdpRunApi, createAwdpRunApi } from "./api/awdpRuns.js";
import {
	type ConnectSseOptions,
	type SseConnection,
	connectSse,
} from "./sse/connectSse.js";
import { type SseEvent, type SseParser, createSseParser } from "./sse/parser.js";
import {
	type FloatCTFAuthScope,
	type FloatCTFClientOptions,
	type FloatCTFHttpClient,
	createFloatCTFTransport,
	resolveBaseUrls,
} from "./transport.js";

export type {
	FloatCTFAuthScope,
	FloatCTFClientOptions,
	FloatCTFHttpClient,
	FloatCTFRequestConfig,
	FloatCTFTransport,
	UnauthorizedContext,
} from "./transport.js";

export type { AdminApi, ServiceApi } from "./api/index.js";
export type { AwdAdminApi, AwdPlayerApi } from "./api/awd.js";
export type { AwdpAdminApi, AwdpPlayerApi } from "./api/awdp.js";
export type { AwdpRunApi } from "./api/awdpRuns.js";

/** SSE 连接选项：`url` 可以是绝对地址，也可以是**相对本实例 base URL** 的路径。 */
export type FloatCTFSseOptions = Omit<ConnectSseOptions, "url" | "getToken"> & {
	/** 相对路径（如 `/events/<id>/awd/stream`）或绝对 URL。 */
	url: string;
	/** 覆盖默认 token 来源（默认：选手端用 `getUserToken`，管理端用 `getAdminToken`）。 */
	getToken?: () => string | null;
};

export interface FloatCTFClient {
	/** 选手端 API base URL（权威值，等于请求真正发往的地址）。 */
	readonly baseUrl: string;
	/** 管理端 API base URL（权威值）。 */
	readonly adminBaseUrl: string;
	/** 底层 axios 实例（逃生舱：自定义拦截器/上传进度/流式响应）。 */
	readonly transport: {
		readonly service: AxiosInstance;
		readonly admin: AxiosInstance;
	};
	/** 本实例的选手端 HTTP handle（领域模块的注入目标）。 */
	readonly serviceHttp: FloatCTFHttpClient;
	/** 本实例的管理端 HTTP handle。 */
	readonly adminHttp: FloatCTFHttpClient;
	/** 领域门面（全部绑定到本实例的 handle）。 */
	readonly service: ServiceApi;
	readonly admin: AdminApi;
	readonly awd: {
		readonly player: AwdPlayerApi;
		readonly admin: AwdAdminApi;
	};
	readonly awdp: {
		readonly player: AwdpPlayerApi;
		readonly admin: AwdpAdminApi;
		readonly runs: AwdpRunApi;
	};
	/** 实时传输（fetch-based SSE，Bearer 走 Authorization 头）。 */
	readonly sse: {
		/** 默认使用**本实例**的选手端 base URL 与 token 来源。 */
		connect(options: FloatCTFSseOptions): SseConnection;
		/** 管理端 SSE（admin base URL + admin token）。 */
		connectAdmin(options: FloatCTFSseOptions): SseConnection;
		createParser(): SseParser;
	};
}

/** 把相对路径解析到指定 base URL 上（绝对 URL 原样返回）。 */
export function resolveSseUrl(baseUrl: string, url: string): string {
	if (/^[a-z][a-z0-9+.-]*:\/\//i.test(url)) {
		return url;
	}
	const path = url.startsWith("/") ? url : `/${url}`;
	return `${baseUrl.replace(/\/+$/, "")}${path}`;
}

/**
 * 创建 FloatCTF 客户端。
 *
 * 返回的对象**完全独立**：不写入任何模块级状态。不需要"解绑"或测试用全局重置；
 * 丢弃引用即可，其它客户端不受影响。
 */
export function createFloatCTFClient(
	options: FloatCTFClientOptions = {},
): FloatCTFClient {
	const { baseUrl, adminBaseUrl } = resolveBaseUrls(options);

	const serviceTransport = createFloatCTFTransport("user", options, baseUrl);
	const adminTransport = createFloatCTFTransport("admin", options, adminBaseUrl);

	const serviceApi = createServiceApi(serviceTransport.http);
	const adminApi = createAdminApi(adminTransport.http);

	const connect = (
		scope: FloatCTFAuthScope,
		sseOptions: FloatCTFSseOptions,
	): SseConnection => {
		const transport = scope === "user" ? serviceTransport : adminTransport;
		const defaultToken =
			scope === "user"
				? (): string | null => options.getUserToken?.() ?? null
				: (): string | null => options.getAdminToken?.() ?? null;
		return connectSse({
			...sseOptions,
			url: resolveSseUrl(transport.baseUrl, sseOptions.url),
			getToken: sseOptions.getToken ?? defaultToken,
		});
	};

	return {
		baseUrl,
		adminBaseUrl,
		transport: {
			service: serviceTransport.instance,
			admin: adminTransport.instance,
		},
		serviceHttp: serviceTransport.http,
		adminHttp: adminTransport.http,
		service: serviceApi,
		admin: adminApi,
		awd: {
			player: serviceApi.awd,
			admin: adminApi.awd,
		},
		awdp: {
			player: createAwdpPlayerApi(serviceTransport.http),
			admin: createAwdpAdminApi(adminTransport.http),
			runs: createAwdpRunApi(serviceTransport.http),
		},
		sse: {
			connect: (sseOptions) => connect("user", sseOptions),
			connectAdmin: (sseOptions) => connect("admin", sseOptions),
			createParser: createSseParser,
		},
	};
}

export type { SseConnection, SseEvent, SseParser, ConnectSseOptions };
