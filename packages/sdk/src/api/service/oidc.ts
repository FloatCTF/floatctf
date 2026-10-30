import type { FloatCTFHttpClient, UniResponse } from "../../transport.js";
import type {
	CreateOidcClientInput,
	OidcAuthorizationDto,
	OidcClientDto,
	OidcClientInput,
	OidcClientWithSecretDto,
	OidcProviderInfo,
	SsoBridgeResult,
} from "../../types/oidc.js";

/**
 * FloatCTF 作为 OpenID Provider 的玩家侧能力：
 * - SSO 会话衔接（平台 JWT → 服务端 HttpOnly Cookie；登出撤销）
 * - 开发者中心（我的 OIDC 应用）
 * - 已授权应用（查看 / 撤销）
 *
 * 浏览器 Cookie 由服务端下发，SDK 不读写 Cookie / localStorage。
 */
export function createOidcServiceApi(http: FloatCTFHttpClient) {
	const enc = encodeURIComponent;
	return {
		/** 用当前平台 JWT 建立 SSO 会话（幂等）。OIDC 未启用时返回 404。 */
		establishSso: async (): Promise<UniResponse<SsoBridgeResult>> => {
			const res = await http.post("/users/sso");
			return res.data;
		},
		/** 撤销当前浏览器的 SSO 会话（登出时调用；OIDC 未启用时也成功）。 */
		endSso: async (): Promise<UniResponse<null>> => {
			const res = await http.delete("/users/sso");
			return res.data;
		},
		provider: async (): Promise<UniResponse<OidcProviderInfo>> => {
			const res = await http.get("/oidc/provider");
			return res.data;
		},
		listClients: async (): Promise<UniResponse<OidcClientDto[]>> => {
			const res = await http.get("/oidc/clients");
			return res.data;
		},
		createClient: async (
			input: CreateOidcClientInput,
		): Promise<UniResponse<OidcClientWithSecretDto>> => {
			const res = await http.post("/oidc/clients", input);
			return res.data;
		},
		getClient: async (clientId: string): Promise<UniResponse<OidcClientDto>> => {
			const res = await http.get(`/oidc/clients/${enc(clientId)}`);
			return res.data;
		},
		updateClient: async (
			clientId: string,
			input: OidcClientInput,
		): Promise<UniResponse<OidcClientDto>> => {
			const res = await http.patch(`/oidc/clients/${enc(clientId)}`, input);
			return res.data;
		},
		/** 轮换 secret；`revokePrevious` 为 true 时旧 secret 立即失效。新 secret 只返回这一次。 */
		rotateSecret: async (
			clientId: string,
			revokePrevious = false,
		): Promise<UniResponse<OidcClientWithSecretDto>> => {
			const res = await http.post(`/oidc/clients/${enc(clientId)}/secret`, {
				revoke_previous: revokePrevious,
			});
			return res.data;
		},
		disableClient: async (clientId: string): Promise<UniResponse<OidcClientDto>> => {
			const res = await http.post(`/oidc/clients/${enc(clientId)}/disable`);
			return res.data;
		},
		enableClient: async (clientId: string): Promise<UniResponse<OidcClientDto>> => {
			const res = await http.post(`/oidc/clients/${enc(clientId)}/enable`);
			return res.data;
		},
		deleteClient: async (clientId: string): Promise<UniResponse<null>> => {
			const res = await http.delete(`/oidc/clients/${enc(clientId)}`);
			return res.data;
		},
		listAuthorizations: async (): Promise<UniResponse<OidcAuthorizationDto[]>> => {
			const res = await http.get("/oidc/authorizations");
			return res.data;
		},
		/** 撤销授权：该应用已签发的 access token 立即失效（不会退出应用自身的登录）。 */
		revokeAuthorization: async (clientId: string): Promise<UniResponse<null>> => {
			const res = await http.delete(`/oidc/authorizations/${enc(clientId)}`);
			return res.data;
		},
	};
}

export type OidcServiceApi = ReturnType<typeof createOidcServiceApi>;
