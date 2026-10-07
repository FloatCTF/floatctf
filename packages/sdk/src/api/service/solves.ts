import type { JeopardyChallengeSolves as ChallengeSolves } from "../../entity/index.js";
import type { TopUser } from "../../types/top.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";

/**
 * 后端 GET /solves 返回的 DTO：challenge_solves 表字段（serde flatten）+ 解题者信息。
 */
export type SolveResult = ChallengeSolves & {
	nickname: string;
	avatar?: string;
	challenge_name: string;
};

export function createSolveServiceApi(http: FloatCTFHttpClient) {
	return {
	    fetch: async (
	        params: QueryParams = {},
	    ): Promise<UniResponse<SolveResult[]>> => {
	        const res = await http.get("/solves", { params });
	        return res.data;
	    },
	    getTop15Users: async (): Promise<UniResponse<TopUser[]>> => {
	        const res = await http.get("/solves/top15users");
	        return res.data;
	    },
};
}

export type SolveServiceApi = ReturnType<typeof createSolveServiceApi>;
