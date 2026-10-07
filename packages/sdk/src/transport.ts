/**
 * HTTP 传输层：**每个客户端实例各自拥有**的 Axios 实例 + Bearer 认证注入 +
 * 错误归一化 + 401 回调。
 *
 * **这个模块是 SDK 与 UI 的分界线**（替换旧的 `apps/web/src/api/axios.ts`）：
 *
 * | 机制 | 归属 |
 * |------|------|
 * | baseURL、Authorization 头注入、错误解析、401 判定 | SDK（本模块） |
 * | token 从哪来（Zustand / localStorage / 内存） | 前端（`getUserToken` / `getAdminToken` 回调） |
 * | 401 之后做什么（跳哪个路由 / 清哪个 store / 弹什么提示） | 前端（`onUnauthorized` 回调） |
 *
 * SDK **绝不** import router / Zustand / localStorage / 任何登录页 URL。
 *
 * ## 实例隔离（重要）
 *
 * 本模块**没有**任何模块级可变状态：不存在"当前绑定的 transport"。
 * `createFloatCTFTransport()` 每次调用都返回**独立**的 axios 实例与 handle，
 * 因此：
 *
 * ```ts
 * const a = createFloatCTFClient({ baseUrl: "https://a.example/api", getUserToken: () => "A" });
 * const b = createFloatCTFClient({ baseUrl: "https://b.example/api", getUserToken: () => "B" });
 * await b.service.events.fetch();   // 用的是 b 的 baseUrl + B
 * await a.service.events.fetch();   // 仍然是 a 的 baseUrl + A，创建顺序无关
 * ```
 */

import axios, {
	type AxiosInstance,
	type AxiosRequestConfig,
	type AxiosResponse,
} from "axios";

import type { FloatCTFError } from "./errors.js";
import { toFloatCTFError } from "./errors.js";

export type { QueryParams, UniResponse } from "./protocol.js";

/** 认证作用域：选手端 / 管理端各自独立携带 token（互不影响）。 */
export type FloatCTFAuthScope = "user" | "admin";

/** 默认选手端 base URL。 */
export const DEFAULT_API_BASE_URL = "/api";

export interface UnauthorizedContext {
	scope: FloatCTFAuthScope;
	status: number;
	error: FloatCTFError;
}

/**
 * 两个 axios 实例共用的默认配置。
 *
 * 刻意**不允许** `baseURL`：客户端自己的 `baseUrl` / `adminBaseUrl` 是唯一权威来源，
 * 否则 `client.baseUrl` 报告一个地址、请求实际发到另一个地址（静默串号）。
 */
export type FloatCTFRequestConfig = Omit<AxiosRequestConfig, "baseURL">;

export interface FloatCTFClientOptions {
	/** 选手端 API base URL，默认 `/api`。 */
	baseUrl?: string;
	/** 管理端 API base URL，默认 `${baseUrl}/admin`。 */
	adminBaseUrl?: string;
	/** 取当前选手 token（同步、纯读）。默认恒为 `null`。 */
	getUserToken?: () => string | null | undefined;
	/** 取当前管理员 token（同步、纯读）。默认恒为 `null`。 */
	getAdminToken?: () => string | null | undefined;
	/**
	 * 收到 401 时调用。前端在此决定清理 token / 跳转登录页 / 提示用户。
	 * SDK 自身不跳转、不写 localStorage。
	 */
	onUnauthorized?: (context: UnauthorizedContext) => void;
	/** 每次请求失败时调用（已归一化的错误）。 */
	onError?: (error: FloatCTFError, scope: FloatCTFAuthScope) => void;
	/** 附加到两个 axios 实例的默认配置（如 `withCredentials`、`timeout`）。 */
	requestConfig?: FloatCTFRequestConfig;
}

/**
 * 暴露给领域 API 模块的 HTTP handle。
 *
 * 泛型签名**逐字对齐 axios**（含 `R = AxiosResponse<T>`）：这样 `return http.get(...)`
 * 这类写法能继续按上下文返回类型推断 `R`，迁移前后的类型行为完全一致。
 */
export interface FloatCTFHttpClient {
	// biome-ignore lint/suspicious/noExplicitAny: 与 axios 的签名逐字对齐（含 D = any）
	get<T = any, R = AxiosResponse<T>, D = any>(
		url: string,
		config?: AxiosRequestConfig<D>,
	): Promise<R>;
	// biome-ignore lint/suspicious/noExplicitAny: 同上
	delete<T = any, R = AxiosResponse<T>, D = any>(
		url: string,
		config?: AxiosRequestConfig<D>,
	): Promise<R>;
	// biome-ignore lint/suspicious/noExplicitAny: 同上
	post<T = any, R = AxiosResponse<T>, D = any>(
		url: string,
		data?: D,
		config?: AxiosRequestConfig<D>,
	): Promise<R>;
	// biome-ignore lint/suspicious/noExplicitAny: 同上
	put<T = any, R = AxiosResponse<T>, D = any>(
		url: string,
		data?: D,
		config?: AxiosRequestConfig<D>,
	): Promise<R>;
	// biome-ignore lint/suspicious/noExplicitAny: 同上
	patch<T = any, R = AxiosResponse<T>, D = any>(
		url: string,
		data?: D,
		config?: AxiosRequestConfig<D>,
	): Promise<R>;
	/** 底层 axios 实例（需要拦截器/流式响应时的逃生舱）。 */
	readonly instance: AxiosInstance;
}

/** 一个作用域的完整传输：权威 base URL + axios 实例 + handle。 */
export interface FloatCTFTransport {
	readonly scope: FloatCTFAuthScope;
	/** 权威 base URL（已去掉尾部斜杠）。 */
	readonly baseUrl: string;
	/** 底层 axios 实例。 */
	readonly instance: AxiosInstance;
	/** 领域模块使用的 handle（与本 transport 一一对应）。 */
	readonly http: FloatCTFHttpClient;
}

/** 去掉尾部斜杠，避免拼出 `//events` 这类路径。 */
export function normalizeBaseUrl(url: string): string {
	return url.replace(/\/+$/, "");
}

function attachInterceptors(
	instance: AxiosInstance,
	scope: FloatCTFAuthScope,
	options: FloatCTFClientOptions,
): void {
	const tokenGetter =
		scope === "user" ? options.getUserToken : options.getAdminToken;

	instance.interceptors.request.use((config) => {
		const token = tokenGetter?.();
		if (token) {
			config.headers.Authorization = `Bearer ${token}`;
		}
		return config;
	});

	instance.interceptors.response.use(
		(response) => response,
		(error: unknown) => {
			// 网络层失败（超时 / 连接中断）没有 response：归一化必须容错，
			// 否则真正的错误会被 TypeError 覆盖成 "reading 'status'"。
			const normalized = toFloatCTFError(error);
			if (normalized.httpStatus === 401) {
				options.onUnauthorized?.({
					scope,
					status: 401,
					error: normalized,
				});
			}
			options.onError?.(normalized, scope);
			return Promise.reject(normalized);
		},
	);
}

function makeHandle(instance: AxiosInstance): FloatCTFHttpClient {
	return {
		get: (url, config) => instance.get(url, config),
		delete: (url, config) => instance.delete(url, config),
		post: (url, data, config) => instance.post(url, data, config),
		put: (url, data, config) => instance.put(url, data, config),
		patch: (url, data, config) => instance.patch(url, data, config),
		get instance() {
			return instance;
		},
	};
}

/**
 * 创建一个作用域的传输（选手端或管理端）。
 *
 * 返回的对象完全独立：不写入任何模块级状态，因此可以同时存在任意多个客户端。
 */
export function createFloatCTFTransport(
	scope: FloatCTFAuthScope,
	options: FloatCTFClientOptions,
	baseUrl: string,
): FloatCTFTransport {
	const normalized = normalizeBaseUrl(baseUrl);
	// `baseURL` 放在 requestConfig **之后**：客户端配置是唯一权威，
	// 即使调用方通过 `as any` 之类的途径塞了 baseURL 也会被覆盖。
	const instance = axios.create({
		...options.requestConfig,
		baseURL: normalized,
	});
	attachInterceptors(instance, scope, options);
	return {
		scope,
		baseUrl: normalized,
		instance,
		http: makeHandle(instance),
	};
}

/**
 * 解析客户端选项里的两个 base URL。
 *
 * 单独导出：`createFloatCTFClient()` 与实际建连都要用同一套规则，
 * 避免 "client.baseUrl 报告的值" 与 "请求真正发往的地址" 不一致。
 */
export function resolveBaseUrls(options: FloatCTFClientOptions): {
	baseUrl: string;
	adminBaseUrl: string;
} {
	const baseUrl = normalizeBaseUrl(options.baseUrl ?? DEFAULT_API_BASE_URL);
	const adminBaseUrl = normalizeBaseUrl(
		options.adminBaseUrl ?? `${baseUrl}/admin`,
	);
	return { baseUrl, adminBaseUrl };
}
