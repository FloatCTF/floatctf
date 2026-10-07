import { type UniResponse, admin_api } from "../../transport.js";
import type { SystemInformation } from "../../types/systemInformation.js";

export const systemAdminApi = {
	monitor: async (): Promise<UniResponse<SystemInformation>> => {
		const response = await admin_api.get("/system/monitor");
		return response.data;
	},
	version: async (): Promise<UniResponse<string>> => {
		const response = await admin_api.get("/system/version");
		return response.data;
	},
};
