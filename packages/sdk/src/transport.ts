/**
 * HTTP 传输层：共享的 Axios 实例 + Bearer 认证注入 + 错误归一化 + 401 回调。
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
 * ## 关于模块级 handle
 *
 * `service_api` / `admin_api` 是"待绑定的 HTTP handle"：API 模块直接 import 它们，
 * 由 `createFloatCTFClient()` 在启动时绑定真实 axios 实例。这样一个页面只会有一个
 * 客户端（bootstrap 每次加载只挂载一个前端），而 **认证来源与 UI 反应仍然由调用方注入**。
 * 未绑定就调用会抛出可读错误，而不是静默发到错误地址。
 */

import axios, {
	type AxiosInstance,
	type AxiosRequestConfig,
	type AxiosResponse,
} from "axios";

import { FloatCTFError, toFloatCTFError } from "./errors.js";

export type { QueryParams, UniResponse } from "./protocol.js";

/** 认证作用域：选手端 / 管理端各自独立携带 token（互不影响）。 */
export type FloatCTFAuthScope = "user" | "admin";

export interface UnauthorizedContext {
	scope: FloatCTFAuthScope;
	status: number;
	error: FloatCTFError;
}

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
	requestConfig?: AxiosRequestConfig;
}

/**
 * 绑定后暴露给 API 模块的 HTTP handle。
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

interface Binding {
	service: AxiosInstance | null;
	admin: AxiosInstance | null;
}

const binding: Binding = { service: null, admin: null };

function unboundError(scope: FloatCTFAuthScope): FloatCTFError {
	return new FloatCTFError({
		message: `FloatCTF ${scope} HTTP client is not configured. Call createFloatCTFClient() before issuing API requests.`,
		kind: "unknown",
	});
}

function createHandle(scope: FloatCTFAuthScope): FloatCTFHttpClient {
	const current = (): AxiosInstance | null =>
		scope === "user" ? binding.service : binding.admin;
	const require = (): AxiosInstance => {
		const instance = current();
		if (!instance) throw unboundError(scope);
		return instance;
	};
	/**
	 * 未绑定时返回**被拒绝的 Promise** 而不是同步抛错：
	 * 所有 API 方法都必须保持 axios 的"总是返回 Promise"语义，否则调用方的
	 * `.catch()` 会漏掉这个错误，变成未捕获异常。
	 */
	const run = <T>(fn: (instance: AxiosInstance) => Promise<T>): Promise<T> => {
		const instance = current();
		if (!instance) return Promise.reject(unboundError(scope));
		return fn(instance);
	};
	const handle: FloatCTFHttpClient = {
		get(url, config) {
			return run((instance) => instance.get(url, config));
		},
		delete(url, config) {
			return run((instance) => instance.delete(url, config));
		},
		post(url, data, config) {
			return run((instance) => instance.post(url, data, config));
		},
		put(url, data, config) {
			return run((instance) => instance.put(url, data, config));
		},
		patch(url, data, config) {
			return run((instance) => instance.patch(url, data, config));
		},
		get instance() {
			return require();
		},
	};
	return handle;
}

/** 选手端 HTTP handle（由 `createFloatCTFClient()` 绑定）。 */
export const service_api: FloatCTFHttpClient = createHandle("user");

/** 管理端 HTTP handle（由 `createFloatCTFClient()` 绑定）。 */
export const admin_api: FloatCTFHttpClient = createHandle("admin");

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

/**
 * 构建并绑定两个 axios 实例。
 *
 * @internal 由 `createFloatCTFClient()` 调用；单独使用请优先用后者。
 */
export function bindHttpClients(options: FloatCTFClientOptions = {}): {
	service: AxiosInstance;
	admin: AxiosInstance;
	dispose: () => void;
} {
	const baseUrl = (options.baseUrl ?? "/api").replace(/\/+$/, "");
	const adminBaseUrl = (options.adminBaseUrl ?? `${baseUrl}/admin`).replace(/\/+$/, "");

	const service = axios.create({ baseURL: baseUrl, ...options.requestConfig });
	const admin = axios.create({ baseURL: adminBaseUrl, ...options.requestConfig });
	attachInterceptors(service, "user", options);
	attachInterceptors(admin, "admin", options);

	binding.service = service;
	binding.admin = admin;

	return {
		service,
		admin,
		dispose: () => {
			if (binding.service === service) binding.service = null;
			if (binding.admin === admin) binding.admin = null;
		},
	};
}

/** 测试用：解除绑定，避免用例之间互相污染。 */
export function resetHttpClientsForTests(): void {
	binding.service = null;
	binding.admin = null;
}
