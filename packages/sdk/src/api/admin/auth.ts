import type { FloatCTFHttpClient } from "../../transport.js";
import type { UniResponse } from "../../transport.js";

/**
 * 管理端登录（v2：与选手端**同一个** `/api/session`）。
 *
 * 注意传入的是**选手端**（service）handle，而不是 admin handle：
 * Wave 10 删除了 `/api/admin/session` 这个过渡别名，`/api/session` 是唯一登录入口。
 * 之所以仍保留 `admin.login`，只是为了让既有调用点不必改形状 ——
 * 它现在就是 service 登录的别名，两者不可能再分叉。
 *
 * 「这个账号能不能进管理端」不再由**登录端点**回答：登录只回答"你是谁"，
 * 能不能管理由 `/me` 的权限或管理端请求的 403 回答（授权与认证分离）。
 */
export function createAdminLoginFn(http: FloatCTFHttpClient) {
	return async ({
		username,
		password,
	}: {
		username: string;
		password: string;
	}): Promise<UniResponse<string>> => {
		const response = await http.post("/session", { username, password });
		return response.data;
	};
}

export type AdminLoginFn = ReturnType<typeof createAdminLoginFn>;
