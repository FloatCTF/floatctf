import type {
	OidcClientStatus,
	OidcClientType,
	OidcTokenAuthMethod,
} from "../entity/sea_orm_active_enums.js";

/** OIDC 应用视图（`/api/oidc/clients*`）。永不包含 secret 或其哈希。 */
export type OidcClientDto = {
	client_id: string;
	name: string;
	description?: string | null;
	homepage_url?: string | null;
	logo_url?: string | null;
	client_type: OidcClientType;
	token_endpoint_auth_method: OidcTokenAuthMethod;
	redirect_uris: string[];
	allowed_scopes: string[];
	/** RP-Initiated Logout 允许的退出后回跳地址（逐字精确匹配）。 */
	post_logout_redirect_uris: string[];
	/** Back-Channel Logout 接收地址（只在用户明确「退出所有关联应用」时投递）。 */
	backchannel_logout_uri?: string | null;
	require_pkce: boolean;
	status: OidcClientStatus;
	status_reason?: string | null;
	has_secret: boolean;
	secret_rotated_at?: string | null;
	/** 轮换后旧 secret 的失效时间；为空表示没有仍有效的旧 secret。 */
	previous_secret_expires_at?: string | null;
	/** 当前有效授权的用户数。 */
	active_authorizations: number;
	created_at: string;
	updated_at: string;
};

/** 创建 / 轮换结果：`client_secret` 只在这一次响应中出现（公开客户端为 null）。 */
export type OidcClientWithSecretDto = {
	client: OidcClientDto;
	client_secret: string | null;
};

/** 所有者可编辑字段（PATCH 为整体替换）。 */
export type OidcClientInput = {
	name: string;
	description?: string | null;
	homepage_url?: string | null;
	logo_url?: string | null;
	redirect_uris: string[];
	scopes: string[];
	/** 省略 = 保持原值（PATCH 为整体替换，但这两个字段例外）。 */
	post_logout_redirect_uris?: string[];
	/** 省略 = 保持原值；空字符串 = 清除。 */
	backchannel_logout_uri?: string;
};

export type CreateOidcClientInput = OidcClientInput & {
	client_type: OidcClientType;
	token_endpoint_auth_method?: OidcTokenAuthMethod;
};

/** 接入所需的 OP 公开信息（开发者中心"复制配置"）。 */
export type OidcProviderInfo = {
	issuer: string;
	discovery_url: string;
	authorization_endpoint: string;
	token_endpoint: string;
	userinfo_endpoint: string;
	jwks_uri: string;
	scopes_supported: string[];
	require_client_approval: boolean;
	allow_loopback_redirect_uris: boolean;
	/** 部署开启标准退出时的 `end_session_endpoint`。 */
	end_session_endpoint?: string | null;
	/** 部署是否支持 Back-Channel Logout。 */
	backchannel_logout_supported?: boolean;
};

/** 当前用户已授权的第三方应用。 */
export type OidcAuthorizationDto = {
	client_id: string;
	client_name: string;
	homepage_url?: string | null;
	client_status: OidcClientStatus;
	scopes: string[];
	granted_at: string;
	updated_at: string;
	last_used_at?: string | null;
};

/** 管理员审核视图。 */
export type OidcAdminClientDto = OidcClientDto & {
	owner_user_id: string;
	owner_username?: string | null;
	reviewed_at?: string | null;
};

/** `POST /api/users/sso` 结果。 */
export type SsoBridgeResult = {
	/** false = 已有同用户的有效 SSO 会话（幂等）。 */
	established: boolean;
};
