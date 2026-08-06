import type { Instances } from "@/entity";
import { type QueryParams, type UniResponse, admin_api } from "@/api/axios";

export const instanceAdminApi = {
    fetch: async (
        params: QueryParams = {},
    ): Promise<UniResponse<Instances[]>> => {
        const res = await admin_api.get("/instances", { params });
        return res.data;
    },
};
