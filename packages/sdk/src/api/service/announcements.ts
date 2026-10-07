import type { Announcements } from "../../entity/index.js";
import { type QueryParams, type UniResponse, service_api } from "../../transport.js";

export const announcementServiceApi = {
    fetch: async (
        params: QueryParams = {},
    ): Promise<UniResponse<Announcements[]>> => {
        const res = await service_api.get("/announcements", { params });
        return res.data;
    },
};
