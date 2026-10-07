import type { Logs } from "../../entity/index.js";
import { type QueryParams, type UniResponse, admin_api } from "../../transport.js";

export const logsAdminApi = {
    fetch: async (params: QueryParams = {}): Promise<UniResponse<Logs[]>> => {
        const res = await admin_api.get("/logs", { params });
        return res.data;
    },
};
