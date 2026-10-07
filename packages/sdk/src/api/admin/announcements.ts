import type { Announcements } from "../../entity/index.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";

export function createAnnouncementAdminApi(http: FloatCTFHttpClient) {
	return {
	    fetch: async (
	        params: QueryParams = {},
	    ): Promise<UniResponse<Announcements[]>> => {
	        const res = await http.get("/announcements", { params });
	        return res.data;
	    },
	    create: async (
	        announcement: Partial<Announcements>,
	    ): Promise<UniResponse<Announcements>> => {
	        const res = await http.post("/announcements", announcement);
	        return res.data;
	    },
	    remove: async (id_list: string[]): Promise<UniResponse<number>> => {
	        const res = await http.delete("/announcements", {
	            data: { id_list },
	        });
	        return res.data;
	    },
	    patch: async (
	        announcement: Partial<Announcements>,
	    ): Promise<UniResponse<Announcements>> => {
	        const res = await http.patch(
	            `/announcements/${announcement.id}`,
	            announcement,
	        );
	        return res.data;
	    },
};
}

export type AnnouncementAdminApi = ReturnType<typeof createAnnouncementAdminApi>;
