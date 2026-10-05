import type { AxiosError } from "axios";

/** axios 在无业务文案时抛出的通用文本（对用户无意义，需被替换成中文提示）。 */
const GENERIC_AXIOS_TEXT = /^Request failed with status code \d+$/i;

/**
 * 把接口错误整理成用户能看懂的中文文案。
 *
 * 优先级：调用方覆盖（如登录页把 401 解释为「用户名或密码错误」）→ 后端业务文案
 * （4xx/5xx 且不是 axios 通用文本）→ 按 HTTP 状态码的中文兜底 → 网络异常提示。
 * 后端原始英文/数据库文本不会被直接展示给用户。
 */
export function formatApiError(
	error: unknown,
	overrides?: Record<number, string>,
): string {
	const axiosError = error as AxiosError<{ message?: string }> | undefined;
	const status = axiosError?.response?.status;
	if (status && overrides?.[status]) return overrides[status];

	const serverMessage = axiosError?.response?.data?.message?.trim();
	if (serverMessage && !GENERIC_AXIOS_TEXT.test(serverMessage)) {
		return serverMessage;
	}

	switch (status) {
		case 400:
			return "请求参数有误，请检查后重试";
		case 401:
			return "登录状态已失效，请重新登录";
		case 403:
			return "没有权限执行该操作";
		case 404:
			return "请求的资源不存在或已被删除";
		case 409:
			return "操作冲突，请刷新后重试";
		case 422:
			return "提交的内容不符合要求，请检查后重试";
		case 429:
			return "操作过于频繁，请稍后再试";
		default:
			break;
	}

	if (typeof status === "number") {
		if (status >= 500) return "服务器内部错误，请稍后重试或联系管理员";
		return "操作失败，请稍后重试";
	}

	// 无 response：超时 / 连接中断 / 请求被取消。
	const message = (error as Error | undefined)?.message ?? "";
	if (/timeout/i.test(message)) return "请求超时，请稍后重试";
	return "网络异常，请检查网络连接后重试";
}
