import { describe, expect, it } from "vitest";

import { createFloatCTFClient } from "../client";

/**
 * OIDC Provider 相关 SDK 方法的 HTTP 契约：方法、路径（client_id 需 URL 编码）、请求体。
 * 后端路由见 apps/api/src/modules/identity/oidc/mod.rs。
 */

interface Hit {
	method?: string;
	url?: string;
	baseURL?: string;
	data?: unknown;
	params?: unknown;
	authorization?: string;
}

function capture(instance: { defaults: { adapter?: unknown } }, hits: Hit[]) {
	instance.defaults.adapter = async (config: Record<string, unknown>) => {
		const headers = (config.headers ?? {}) as Record<string, unknown>;
		hits.push({
			method: (config.method as string | undefined)?.toUpperCase(),
			url: config.url as string,
			baseURL: config.baseURL as string,
			data: config.data ? JSON.parse(config.data as string) : undefined,
			params: config.params,
			authorization: headers.Authorization as string | undefined,
		});
		return { status: 200, data: { code: 0, message: "OK", data: null }, statusText: "", headers: {}, config };
	};
}

describe("service.oidc", () => {
	it("maps developer-center, authorization and SSO calls to the API contract", async () => {
		const client = createFloatCTFClient({ baseUrl: "/api", getUserToken: () => "USER_JWT" });
		const hits: Hit[] = [];
		capture(client.transport.service, hits);
		const id = "fctf_a/b";
		const oidc = client.service.oidc;

		await oidc.establishSso();
		await oidc.endSso();
		await oidc.provider();
		await oidc.listClients();
		await oidc.createClient({
			client_type: "confidential" as never,
			name: "App",
			redirect_uris: ["https://app.example/cb"],
			scopes: ["openid"],
		});
		await oidc.getClient(id);
		await oidc.updateClient(id, { name: "App2", redirect_uris: ["https://app.example/cb"], scopes: ["openid"] });
		await oidc.rotateSecret(id, true);
		await oidc.disableClient(id);
		await oidc.enableClient(id);
		await oidc.deleteClient(id);
		await oidc.listAuthorizations();
		await oidc.revokeAuthorization(id);

		expect(hits.map((h) => `${h.method} ${h.url}`)).toEqual([
			"POST /users/sso",
			"DELETE /users/sso",
			"GET /oidc/provider",
			"GET /oidc/clients",
			"POST /oidc/clients",
			"GET /oidc/clients/fctf_a%2Fb",
			"PATCH /oidc/clients/fctf_a%2Fb",
			"POST /oidc/clients/fctf_a%2Fb/secret",
			"POST /oidc/clients/fctf_a%2Fb/disable",
			"POST /oidc/clients/fctf_a%2Fb/enable",
			"DELETE /oidc/clients/fctf_a%2Fb",
			"GET /oidc/authorizations",
			"DELETE /oidc/authorizations/fctf_a%2Fb",
		]);
		expect(hits.every((h) => h.baseURL === "/api" && h.authorization === "Bearer USER_JWT")).toBe(true);
		expect(hits[7].data).toEqual({ revoke_previous: true });
		expect(hits[4].data).toMatchObject({ name: "App", client_type: "confidential" });
	});
});

describe("admin.oidc_clients", () => {
	it("uses the admin transport and review endpoints", async () => {
		const client = createFloatCTFClient({ baseUrl: "/api", getAdminToken: () => "ADMIN_JWT" });
		const hits: Hit[] = [];
		capture(client.transport.admin, hits);
		const admin = client.admin.oidc_clients;
		await admin.fetch("pending" as never);
		await admin.approve("c1");
		await admin.reject("c1", "spam");
		await admin.suspend("c1");
		await admin.reinstate("c1");
		await admin.setRequirePkce("c1", false);
		expect(hits.map((h) => `${h.method} ${h.url}`)).toEqual([
			"GET /oidc/clients",
			"POST /oidc/clients/c1/approve",
			"POST /oidc/clients/c1/reject",
			"POST /oidc/clients/c1/suspend",
			"POST /oidc/clients/c1/reinstate",
			"PATCH /oidc/clients/c1/pkce",
		]);
		expect(hits[0].params).toEqual({ status: "pending" });
		expect(hits[2].data).toEqual({ reason: "spam" });
		expect(hits[5].data).toEqual({ require_pkce: false });
		expect(hits.every((h) => h.baseURL === "/api/admin" && h.authorization === "Bearer ADMIN_JWT")).toBe(true);
	});
});
