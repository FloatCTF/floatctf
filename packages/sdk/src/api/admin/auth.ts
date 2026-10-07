import type { FloatCTFHttpClient } from "../../transport.js";
import type { UniResponse } from "../../transport.js";

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
