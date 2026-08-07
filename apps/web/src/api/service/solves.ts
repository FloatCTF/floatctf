import type { ChallengeSolves } from "@/entity";
import type { TopUser } from "@/routes/service/top";
import { type QueryParams, type UniResponse, service_api } from "../axios";

/**
 * 后端 GET /solves 返回的 DTO：challenge_solves 表字段（serde flatten）+ 解题者信息。
 */
export type SolveResult = ChallengeSolves & {
	nickname: string;
	avatar?: string;
};

export const solveServiceApi = {
    fetch: async (
        params: QueryParams = {},
    ): Promise<UniResponse<SolveResult[]>> => {
        const res = await service_api.get("/solves", { params });
        return res.data;
    },
    getTop15Users: async (): Promise<UniResponse<TopUser[]>> => {
        const res = await service_api.get("/solves/top15users");
        return res.data;
    },
};
