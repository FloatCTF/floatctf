/**
 * 调用约定 —— 把 SDK 的 `UniResponse<T>` 信封转成「成功得到数据 / 失败抛错」的直觉语义。
 *
 * 为什么需要它（三条已核实的 SDK 事实）：
 * 1. 领域方法**返回信封**而不是数据：`await client.service.events.fetch()` 得到
 *    `{ code, message, data?, meta? }`。
 * 2. **HTTP 200 + `code !== 0` 不会被 SDK 拒绝**（传输层从不调用 `floatCTFErrorFromEnvelope`），
 *    因此 `try/catch` 抓不到平台业务失败 —— 必须显式判定。
 * 3. `data?: T` 是可选字段，成功响应也可能没有 `data`。
 *
 * 本模块在 transport 上装一个响应拦截器（`installEnvelopeGuard`），把平台业务失败统一转成
 * `FloatCTFError` rejection；`call` / `callList` 只负责取数据，让页面的 `useQuery` 三态自然成立。
 */

import {
	FloatCTFError,
	UNI_SUCCESS_CODE,
	floatCTFErrorFromEnvelope,
	toFloatCTFError,
	type FloatCTFClient,
	type QueryParams,
	type UniResponse,
} from "@floatctf/sdk";

interface EnvelopeView {
	code?: unknown;
	message?: unknown;
	data?: unknown;
	meta?: unknown;
}

interface ResponseLike {
	data?: unknown;
}

/** axios 实例的结构化视图（不直接依赖 axios 类型，保持依赖面最小）。 */
interface InterceptableTransport {
	interceptors: {
		response: {
			use(
				onFulfilled: (response: ResponseLike) => unknown,
				onRejected?: (error: unknown) => unknown,
			): number;
		};
	};
}

interface UploadableTransport {
	post(
		url: string,
		data?: unknown,
		config?: {
			onUploadProgress?: (event: unknown) => void;
			headers?: Record<string, string>;
		},
	): Promise<ResponseLike>;
	patch(
		url: string,
		data?: unknown,
		config?: {
			onUploadProgress?: (event: unknown) => void;
			headers?: Record<string, string>;
		},
	): Promise<ResponseLike>;
}

function asEnvelope(value: unknown): EnvelopeView | null {
	if (typeof value !== "object" || value === null) return null;
	const record = value as Record<string, unknown>;
	if (typeof record.code === "number") return record;
	// `client.admin.event_users.add` 运行时返回的是 AxiosResponse（信封在 `.data` 下）。
	const inner = record.data;
	if (
		typeof inner === "object" &&
		inner !== null &&
		typeof (inner as Record<string, unknown>).code === "number"
	) {
		return inner as EnvelopeView;
	}
	return null;
}

/**
 * 在客户端两个作用域的 transport 上安装信封守卫。
 * - 平台业务失败（`code !== 0`）→ reject `FloatCTFError{kind:"platform"}`
 * - 传输层失败 → 归一化为 `FloatCTFError`（网络 / HTTP）
 */
export function installEnvelopeGuard(client: FloatCTFClient): void {
	const transports = [
		client.transport.service,
		client.transport.admin,
	] as unknown as InterceptableTransport[];
	for (const transport of transports) {
		transport.interceptors.response.use(
			(response) => {
				const failure = floatCTFErrorFromEnvelope(response?.data);
				if (failure) return Promise.reject(failure);
				return response;
			},
			(error) => Promise.reject(toFloatCTFError(error)),
		);
	}
}

/** 把信封（或 AxiosResponse 包裹的信封）解成数据；失败抛 `FloatCTFError`。 */
export function unwrap<T>(response: unknown, what = "请求"): T {
	const envelope = asEnvelope(response);
	if (!envelope) {
		if (response === undefined || response === null) return undefined as T;
		const direct = (response as ResponseLike).data;
		if (direct !== undefined) return direct as T;
		return response as T;
	}
	if (envelope.code !== UNI_SUCCESS_CODE) {
		throw (
			floatCTFErrorFromEnvelope(envelope) ??
			new FloatCTFError({ message: `${what}失败`, kind: "platform" })
		);
	}
	return (envelope.data ?? undefined) as T;
}

/** 取数据：`const event = await call(client.service.events.get(id), "赛事详情")`。 */
export function call<T>(request: Promise<UniResponse<T>>, what?: string): Promise<T>;
export function call<T>(request: Promise<unknown>, what?: string): Promise<T>;
export async function call<T>(request: Promise<unknown>, what?: string): Promise<T> {
	return unwrap<T>(await request, what);
}

/** 取列表 + 分页 meta：`const { items, meta } = await callList(...)`。 */
export function unwrapList<T>(response: unknown): { items: T[]; meta: QueryParams } {
	const envelope = asEnvelope(response);
	const data = unwrap<T[] | null | undefined>(response);
	return {
		items: Array.isArray(data) ? data : [],
		meta: (envelope?.meta as QueryParams | undefined) ?? {},
	};
}

export function callList<T>(
	request: Promise<UniResponse<T[]>>,
): Promise<{ items: T[]; meta: QueryParams }>;
export function callList<T>(
	request: Promise<unknown>,
): Promise<{ items: T[]; meta: QueryParams }>;
export async function callList<T>(
	request: Promise<unknown>,
): Promise<{ items: T[]; meta: QueryParams }> {
	return unwrapList<T>(await request);
}

/** 允许 `null` 的读取（例如 `awd.admin.getStatus` 在未开 AWD 时返回 null）。 */
export async function callMaybe<T>(request: Promise<unknown>, what?: string): Promise<T | null> {
	const data = unwrap<T | null>(await request, what);
	return data ?? null;
}

/** 无返回值的写操作（`await callVoid(client.service.events.join(id))`）。 */
export async function callVoid(request: Promise<unknown>, what?: string): Promise<void> {
	unwrap<unknown>(await request, what);
}

export interface UploadOptions {
	client: FloatCTFClient;
	/** 选手端（`/api`）或管理端（`/api/admin`）作用域。 */
	scope?: "user" | "admin";
	/** 相对该作用域 base URL 的路径，例如 `/uploads/image` 或 `/weapons/{id}/upload`。 */
	url: string;
	/** multipart 字段名（后端各不相同，写错即 400）。 */
	field: string;
	/**
	 * HTTP 方法。多数上传是 POST，但 `uploads.upload_avatar` 在后端是 **PATCH**
	 * `/uploads/avatar`（见 `packages/sdk/src/api/service/uploads.ts`）。
	 */
	method?: "post" | "patch";
	file: File;
	extra?: Record<string, string>;
	onProgress?: (percent: number) => void;
}

/**
 * 带进度上传。走 `client.transport.*`（受支持的公共逃生舱，AI-FRONTEND-GUIDE §5.4），
 * 因为进度事件是 axios 级能力，领域门面不暴露。
 */
export async function uploadFile<T = unknown>(options: UploadOptions): Promise<T> {
	const {
		client,
		scope = "user",
		url,
		field,
		file,
		extra,
		onProgress,
		method = "post",
	} = options;
	const form = new FormData();
	form.append(field, file);
	for (const [key, value] of Object.entries(extra ?? {})) form.append(key, value);

	const transport = (
		scope === "admin" ? client.transport.admin : client.transport.service
	) as unknown as UploadableTransport;

	const response = await transport[method](url, form, {
		onUploadProgress: (event) => {
			if (!onProgress) return;
			const total = (event as { total?: number }).total;
			const loaded = (event as { loaded?: number }).loaded ?? 0;
			onProgress(total ? Math.min(100, Math.round((loaded / total) * 100)) : 0);
		},
	});
	return unwrap<T>(response, "上传");
}
