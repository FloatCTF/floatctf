import type { Announcements } from "@/entity";
import { type QueryParams, type UniResponse, service_api } from "../axios";

export const announcementServiceApi = {
    fetch: async (
        params: QueryParams = {},
    ): Promise<UniResponse<Announcements[]>> => {
        const res = await service_api.get("/announcements", { params });
        return res.data;
    },
};
