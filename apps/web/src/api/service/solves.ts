import type { ChallengeSolves } from "@/entity";
import type { TopUser } from "@/routes/service/top";
import { type QueryParams, type UniResponse, service_api } from "../axios";

export const solveServiceApi = {
    fetch: async (
        params: QueryParams = {},
    ): Promise<UniResponse<ChallengeSolves[]>> => {
        const res = await service_api.get("/solves", { params });
        return res.data;
    },
    getTop15Users: async (): Promise<UniResponse<TopUser[]>> => {
        const res = await service_api.get("/solves/top15users");
        return res.data;
    },
};
