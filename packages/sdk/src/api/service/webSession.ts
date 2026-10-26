import type { FloatCTFHttpClient, UniResponse } from "../../transport.js";
import type {
	WebLogoutScope,
	WebSessionStatus,
} from "../../types/webSession.js";

/**
 * 主站统一 Cookie 会话（`/api/users/web-session`）。
 *
 * - 状态由服务端判定：`status()` 是唯一的登录态依据（不读 localStorage）。
 * - 状态变更接口都要求 `X-CSRF-Token`：前端通过 `getUserSessionHeaders` 注入
 *   （尚未登录、还没有 CSRF token 时由 `csrfToken` 参数显式传入，例如 `"anonymous"`）。
 * - 所有接口返回最新状态；失败一律抛出，前端必须如实提示（例如「退出未完成」）。
 * - 部署未开启时 `status()` 返回 `mode = "legacy"`，其余接口 404。
 */
export function createWebSessionServiceApi(http: FloatCTFHttpClient) {
	const headers = (csrfToken?: string | null) =>
		csrfToken ? { headers: { "X-CSRF-Token": csrfToken } } : undefined;
	const unwrap = (res: { data: UniResponse<WebSessionStatus> }) =>
		res.data.data as WebSessionStatus;
	return {
		/** 当前浏览器的主站登录态与本浏览器已登录账号。 */
		status: async (): Promise<WebSessionStatus> =>
			unwrap(await http.get("/users/web-session")),
		/** 口令登录：账号加入本浏览器并成为主站当前账号；响应不含令牌。 */
		login: async (
			input: { username: string; password: string },
			csrfToken?: string | null,
		): Promise<WebSessionStatus> =>
			unwrap(
				await http.post("/users/web-session/login", input, headers(csrfToken)),
			),
		/** 从本浏览器唯一可恢复的身份会话恢复主站登录（多个时 409，需要用户选择）。 */
		restore: async (csrfToken?: string | null): Promise<WebSessionStatus> =>
			unwrap(
				await http.post("/users/web-session/restore", {}, headers(csrfToken)),
			),
		/** 切换主站当前账号（必须是本浏览器中仍有效的账号）。 */
		select: async (
			accountId: string,
			csrfToken?: string | null,
		): Promise<WebSessionStatus> =>
			unwrap(
				await http.post(
					"/users/web-session/select",
					{ account_id: accountId },
					headers(csrfToken),
				),
			),
		/** 退出：撤销成功才 resolve；不通知第三方应用。 */
		logout: async (
			scope: WebLogoutScope,
			accountId?: string | null,
			csrfToken?: string | null,
		): Promise<WebSessionStatus> =>
			unwrap(
				await http.post(
					"/users/web-session/logout",
					{ scope, account_id: accountId ?? null },
					headers(csrfToken),
				),
			),
		/**
		 * 把旧版 localStorage 平台 JWT 换成服务端会话（只接受 Bearer）。
		 * 成功后调用方应删除本地旧令牌。
		 */
		migrate: async (
			legacyToken: string,
			csrfToken?: string | null,
		): Promise<WebSessionStatus> =>
			unwrap(
				await http.post("/users/web-session/migrate", {}, {
					headers: {
						Authorization: `Bearer ${legacyToken}`,
						"X-CSRF-Token": csrfToken || "anonymous",
					},
				}),
			),
	};
}

export type WebSessionServiceApi = ReturnType<typeof createWebSessionServiceApi>;
