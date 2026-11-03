/**
 * 错误展示。
 *
 * 平台级要求：错误必须**可见、可读、可恢复**，绝不白屏、绝不静默失败。
 * 文案优先级：后端 `platformMessage`（平台原文）> HTTP 状态语义 > 通用兜底。
 * **不发明业务文案，不猜测后端原因**。
 */
import { FloatCTFError } from "@floatctf/sdk";

export function isFloatCTFError(e: unknown): e is FloatCTFError {
	return e instanceof FloatCTFError;
}

export function isNotFound(e: unknown): boolean {
	return isFloatCTFError(e) && e.httpStatus === 404;
}

export function isForbidden(e: unknown): boolean {
	return isFloatCTFError(e) && e.isForbidden;
}

export function isUnauthorized(e: unknown): boolean {
	return isFloatCTFError(e) && e.isUnauthorized;
}

/** 永远返回一句可以直接显示给用户的话。 */
export function errorText(e: unknown, fallback = "操作失败，请稍后重试"): string {
	if (isFloatCTFError(e)) {
		if (e.platformMessage?.trim()) return e.platformMessage;
		if (e.kind === "network") return "无法连接到服务器，请检查网络后重试";
		switch (e.httpStatus) {
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
				return `服务器错误（${e.httpStatus}），请稍后重试`;
			default:
				return e.displayMessage || fallback;
		}
	}
	if (e instanceof Error && e.message) return e.message;
	return fallback;
}

/**
 * 统一解包 SDK 的 `UniResponse`。
 *
 * SDK 的领域方法返回 `UniResponse<T>`：HTTP 200 但 `code !== 0` 表示**平台业务失败**。
 * 若不解包，界面会把失败当成功（「静默成功」是历史缺陷）。
 */
export function unwrap<T>(
	res: { code?: number; message?: string; data?: T | null },
	action: string,
): T {
	const code = res.code ?? 0;
	if (code !== 0) {
		throw new FloatCTFError({
			message: res.message?.trim() || `${action}失败`,
			kind: "platform",
			code,
			platformMessage: res.message,
		});
	}
	return res.data as T;
}
