import type { Logs } from "../../entity/index.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";

export function createLogsAdminApi(http: FloatCTFHttpClient) {
	return {
	    fetch: async (params: QueryParams = {}): Promise<UniResponse<Logs[]>> => {
	        const res = await http.get("/logs", { params });
	        return res.data;
	    },
};
}

export type LogsAdminApi = ReturnType<typeof createLogsAdminApi>;
