/**
 * FloatCTF 错误模型。
 *
 * 目标（docs/frontend/ARCHITECTURE.md §错误契约）：
 * - 每个前端作者**不必重复解析 Axios 错误**就能拿到：HTTP 状态、平台业务码、
 *   平台文案、原始 cause。
 * - 同时**保持与既有 UI 完全一致的读取方式**（`error.response?.data?.message` /
 *   `error.message`）：迁移期间不要求任何页面改写错误处理，UI 行为零变化。
 *
 * 因此 `FloatCTFError` 既提供类型化字段（`httpStatus` / `code` / `platformMessage` /
 * `original`），也保留 axios 兼容的 `response` 形态。
 */

import { isAxiosError } from "axios";
import { UNI_SUCCESS_CODE } from "./protocol.js";

export type FloatCTFErrorKind =
	/** 服务端返回了 HTTP 响应，但状态码非 2xx。 */
	| "http"
	/** 平台返回了非成功业务码（HTTP 200 但 `code !== 0`）。 */
	| "platform"
	/** 请求未拿到响应：超时 / 连接被拒 / DNS 失败 / 被取消。 */
	| "network"
	/** 无法归类的异常。 */
	| "unknown";

/** 与 axios `response` 兼容的最小形态（既有 UI 读取 `response.data.message`）。 */
export interface FloatCTFErrorResponse {
	status?: number;
	statusText?: string;
	data?: unknown;
	headers?: unknown;
}

export class FloatCTFError extends Error {
	override readonly name = "FloatCTFError";
	readonly kind: FloatCTFErrorKind;
	/** HTTP 状态码（无响应时为 undefined）。 */
	readonly httpStatus?: number;
	/** 平台业务码（若响应体是 `UniResponse`）。 */
	readonly code?: number;
	/** 平台文案（`UniResponse.message`）。 */
	readonly platformMessage?: string;
	/** 原始错误（通常是 axios 错误），用于深度诊断。 */
	readonly original?: unknown;
	/** 是否因认证失败（401）触发；前端据此决定是否跳登录。 */
	readonly unauthorized: boolean;
	/**
	 * 是否因**已认证但不允许**（403）触发。
	 *
	 * 与 `unauthorized` 互斥：403 表示「你已登录，但这件事你不能做」，
	 * 处理后应当**保留**会话，只提示无权限。
	 */
	readonly forbidden: boolean;
	/** 兼容既有 UI 的 axios 形态读取。 */
	readonly response?: FloatCTFErrorResponse;

	constructor(init: {
		message: string;
		kind: FloatCTFErrorKind;
		httpStatus?: number;
		code?: number;
		platformMessage?: string;
		original?: unknown;
		unauthorized?: boolean;
		forbidden?: boolean;
		response?: FloatCTFErrorResponse;
	}) {
		super(init.message);
		this.kind = init.kind;
		this.httpStatus = init.httpStatus;
		this.code = init.code;
		this.platformMessage = init.platformMessage;
		this.original = init.original;
		this.unauthorized = init.unauthorized ?? init.httpStatus === 401;
		this.forbidden = init.forbidden ?? init.httpStatus === 403;
		this.response = init.response;
	}

	/**
	 * 凭据问题：401 —— 应清 token 并重新登录。
	 *
	 * 与 [`isForbidden`] 的区别是 v2 授权契约的核心：
	 * 403 **不是**凭据问题，清 token 跳登录页解决不了它。
	 */
	get isUnauthorized(): boolean {
		return this.unauthorized;
	}

	/**
	 * 已认证但**不允许**（403）：账号被禁用、缺少权限、或对象级授权失败。
	 *
	 * 前端应展示「无权限」提示并**保留**当前会话，
	 * 绝不能像 401 那样清 token 跳登录页（那会让用户以为会话丢了）。
	 */
	get isForbidden(): boolean {
		return this.forbidden;
	}

	/** 最适合展示给用户的文案：平台文案优先，其次 Error.message。 */
	get displayMessage(): string {
		return this.platformMessage?.trim() || this.message;
	}

	toJSON(): Record<string, unknown> {
		return {
			name: this.name,
			kind: this.kind,
			httpStatus: this.httpStatus,
			code: this.code,
			platformMessage: this.platformMessage,
			message: this.message,
			unauthorized: this.unauthorized,
			forbidden: this.forbidden,
		};
	}
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * 把任意异常归类为 {@link FloatCTFError}。
 *
 * 已归一化的错误原样返回（幂等），避免拦截器叠加多层包装。
 */
export function toFloatCTFError(error: unknown): FloatCTFError {
	if (error instanceof FloatCTFError) return error;

	if (isAxiosError(error)) {
		const status = error.response?.status;
		const data = error.response?.data;
		const platformMessage =
			isRecord(data) && typeof data.message === "string" ? data.message : undefined;
		const code =
			isRecord(data) && typeof data.code === "number" ? data.code : undefined;
		const kind: FloatCTFErrorKind = status === undefined ? "network" : "http";
		return new FloatCTFError({
			message: error.message,
			kind,
			...(status !== undefined ? { httpStatus: status } : {}),
			...(code !== undefined ? { code } : {}),
			...(platformMessage !== undefined ? { platformMessage } : {}),
			original: error,
			unauthorized: status === 401,
			...(error.response
				? {
						response: {
							status: error.response.status,
							statusText: error.response.statusText,
							data: error.response.data,
							headers: error.response.headers,
						},
					}
				: {}),
		});
	}

	if (error instanceof Error) {
		return new FloatCTFError({
			message: error.message,
			kind: "unknown",
			original: error,
		});
	}

	return new FloatCTFError({ message: String(error), kind: "unknown", original: error });
}

/**
 * 把非 2xx 之外的"平台业务失败"（HTTP 2xx 但 `code !== 0`）归一化为错误。
 * 返回 `null` 表示这是一个成功的 envelope。
 */
export function floatCTFErrorFromEnvelope(envelope: unknown): FloatCTFError | null {
	if (!isRecord(envelope)) return null;
	const code = envelope.code;
	if (typeof code !== "number" || code === UNI_SUCCESS_CODE) return null;
	const message = typeof envelope.message === "string" ? envelope.message : "platform error";
	return new FloatCTFError({
		message,
		kind: "platform",
		code,
		platformMessage: message,
	});
}
