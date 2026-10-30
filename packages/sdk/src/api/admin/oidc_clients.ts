import type { OidcClientStatus } from "../../entity/sea_orm_active_enums.js";
import type { FloatCTFHttpClient, UniResponse } from "../../transport.js";
import type { OidcAdminClientDto, OidcClientDto } from "../../types/oidc.js";

/** 管理员审核 OIDC 应用（`/api/admin/oidc/clients*`）。 */
export function createOidcClientAdminApi(http: FloatCTFHttpClient) {
	const enc = encodeURIComponent;
	const review =
		(action: "approve" | "reject" | "suspend" | "reinstate") =>
		async (clientId: string, reason?: string): Promise<UniResponse<OidcClientDto>> => {
			const res = await http.post(`/oidc/clients/${enc(clientId)}/${action}`, {
				reason: reason ?? null,
			});
			return res.data;
		};
	return {
		fetch: async (status?: OidcClientStatus): Promise<UniResponse<OidcAdminClientDto[]>> => {
			const res = await http.get("/oidc/clients", { params: status ? { status } : {} });
			return res.data;
		},
		approve: review("approve"),
		reject: review("reject"),
		suspend: review("suspend"),
		reinstate: review("reinstate"),
		/** 仅机密客户端可豁免 PKCE（豁免后授权请求必须带 nonce）。 */
		setRequirePkce: async (
			clientId: string,
			requirePkce: boolean,
		): Promise<UniResponse<OidcClientDto>> => {
			const res = await http.patch(`/oidc/clients/${enc(clientId)}/pkce`, {
				require_pkce: requirePkce,
			});
			return res.data;
		},
	};
}

export type OidcClientAdminApi = ReturnType<typeof createOidcClientAdminApi>;
