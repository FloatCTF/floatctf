/**
 * 主站统一 Cookie 会话（`/api/users/web-session`，服务端 `[oidc].unified_web_session`）。
 *
 * 浏览器认证凭据是服务端下发的 HttpOnly Cookie，**这里的任何字段都不是凭据**：
 * `csrf_token` 只用于放进 `X-CSRF-Token` 请求头；账号 `id` 是本浏览器的会话 id，
 * 只能在持有同一浏览器 Cookie 时用于切换/退出。
 */

/** 主站当前用户。 */
export type WebSessionUser = {
	id: string;
	username: string;
	nickname: string;
	avatar?: string | null;
};

/** 本浏览器中真实有效的一个已登录账号（服务端判定，不是历史记录）。 */
export type WebAccount = {
	/** 会话 id（切换 / 退出时提交）。 */
	id: string;
	user_id: string;
	username: string;
	nickname: string;
	avatar?: string | null;
	/** 是否为主站当前账号。 */
	active: boolean;
};

export type WebSessionStatus = {
	/** `cookie`：统一 Cookie 会话；`legacy`：部署未开启，继续使用平台 JWT。 */
	mode: "cookie" | "legacy";
	authenticated: boolean;
	user: WebSessionUser | null;
	/** 放进 `X-CSRF-Token`；浏览器尚无账号容器时为 null。 */
	csrf_token: string | null;
	accounts: WebAccount[];
	/** 主站未登录且恰好一个账号可自动恢复（`restore()` 会成功）。 */
	auto_restore: boolean;
	max_accounts: number;
	/** 部署是否支持「退出所有关联应用」。 */
	global_logout: boolean;
	/** 只出现在 `logout("global")` 的响应中：每个应用的真实通知结果。 */
	logout_report?: GlobalLogoutReport;
};

/** 一个应用的 Back-Channel 退出通知结果。 */
export type LogoutDelivery = {
	client_id: string;
	client_name: string;
	/** `delivered` 已确认；`pending` 未送达、将自动重试；`failed` 未能确认。 */
	status: "delivered" | "pending" | "failed";
	detail?: string | null;
};

export type GlobalLogoutReport = {
	deliveries: LogoutDelivery[];
	/** 拿到过该会话令牌、但不支持即时退出通知的应用（会话可能仍然有效）。 */
	without_backchannel: string[];
};

/**
 * `account`：退出一个账号（默认当前账号）；`browser`：退出本浏览器全部账号；
 * `global`：退出当前账号并通知关联应用（需用户明确确认；部署开启时可用）。
 */
export type WebLogoutScope = "account" | "browser" | "global";
