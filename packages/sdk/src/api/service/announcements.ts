import type { Announcements } from "../../entity/index.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";

export function createAnnouncementServiceApi(http: FloatCTFHttpClient) {
	return {
	    fetch: async (
	        params: QueryParams = {},
	    ): Promise<UniResponse<Announcements[]>> => {
	        const res = await http.get("/announcements", { params });
	        return res.data;
	    },
};
}

export type AnnouncementServiceApi = ReturnType<typeof createAnnouncementServiceApi>;
