/**
 * 错误展示 —— 平台级要求：错误必须**可见、可读、可恢复**，绝不白屏、绝不静默失败。
 * 文案来源优先级：后端 `UniResponse.message`（`platformMessage`）> HTTP 状态语义 > 通用兜底。
 * 不发明业务文案，不猜测后端原因。
 */

import { FloatCTFError } from "@floatctf/sdk";

export function isFloatCTFError(error: unknown): error is FloatCTFError {
	return error instanceof FloatCTFError;
}

export function isUnauthorized(error: unknown): boolean {
	return isFloatCTFError(error) && (error.unauthorized || error.httpStatus === 401);
}

export function isForbidden(error: unknown): boolean {
	return isFloatCTFError(error) && error.httpStatus === 403;
}

export function isNotFound(error: unknown): boolean {
	return isFloatCTFError(error) && error.httpStatus === 404;
}

export function isNetworkError(error: unknown): boolean {
	return isFloatCTFError(error) && error.kind === "network";
}

/** 面向用户的错误文案。永远返回一句可以直接显示的话。 */
export function errorText(error: unknown, fallback = "操作失败，请稍后重试"): string {
	if (isFloatCTFError(error)) {
		if (error.platformMessage && error.platformMessage.trim().length > 0) {
			return error.platformMessage;
		}
		if (error.kind === "network") {
			return "网络请求失败：无法连接服务器，请检查网络后重试";
		}
		switch (error.httpStatus) {
			case 400:
				return "请求参数不被接受（400）";
			case 401:
				return "登录状态已失效，请重新登录";
			case 403:
				return "没有权限执行该操作（403）";
			case 404:
				return "资源不存在或已被删除（404）";
			case 409:
				return "当前状态与该操作冲突（409），请刷新后重试";
			case 429:
				return "请求过于频繁（429），请稍后再试";
			case 500:
			case 502:
			case 503:
			case 504:
				return `服务器错误（${error.httpStatus}），请稍后重试`;
			default:
				return error.displayMessage || fallback;
		}
	}
	if (error instanceof Error && error.message) return error.message;
	if (typeof error === "string" && error.length > 0) return error;
	return fallback;
}

/** 诊断用细节（错误页「详情」折叠区）：不含 token，只含状态码与平台码。 */
export function errorDetail(error: unknown): string | null {
	if (!isFloatCTFError(error)) {
		if (error instanceof Error) return `${error.name}: ${error.message}`;
		return null;
	}
	const parts: string[] = [`kind=${error.kind}`];
	if (error.httpStatus !== undefined) parts.push(`http=${error.httpStatus}`);
	if (error.code !== undefined) parts.push(`code=${error.code}`);
	if (error.platformMessage) parts.push(`message=${error.platformMessage}`);
	if (error.message && error.message !== error.platformMessage) parts.push(`raw=${error.message}`);
	return parts.join(" · ");
}
