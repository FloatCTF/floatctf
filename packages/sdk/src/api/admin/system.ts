import type { FloatCTFHttpClient } from "../../transport.js";
import type { UniResponse } from "../../transport.js";
import type { SystemInformation } from "../../types/systemInformation.js";

export function createSystemAdminApi(http: FloatCTFHttpClient) {
	return {
		monitor: async (): Promise<UniResponse<SystemInformation>> => {
			const response = await http.get("/system/monitor");
			return response.data;
		},
		version: async (): Promise<UniResponse<string>> => {
			const response = await http.get("/system/version");
			return response.data;
		},
};
}

export type SystemAdminApi = ReturnType<typeof createSystemAdminApi>;
