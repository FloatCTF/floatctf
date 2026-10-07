import type { InstancesDto as Instances } from "./instances.js";
import type {
	ChallengeSets,
	ChallengeWriteup,
	Challenges,
} from "../../entity/index.js";
import type { ChallengeWriteupResult } from "../../types/challengeWriteup.js";
import type { ChallengesListItem } from "../../types/challengeDto.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";

/** 全局 Writeup 列表统一条目（challenge + gamebox 合并；writeup_type 区分类型）。 */
export type UnifiedWriteupResult = {
	id: string;
	writeup_type: "challenge" | "gamebox";
	nickname: string;
	avatar?: string | null;
	email: string;
	content_id: string;
	content_name: string;
	updated_at: string;
};

/** 单个 Writeup 详情统一条目（challenge + gamebox 都能渲染；gamebox 的 id 即 run_id）。 */
export type UnifiedWriteupDetail = {
	id: string;
	writeup_type: "challenge" | "gamebox";
	content_id: string;
	content_name: string;
	category?: string | null;
	nickname: string;
	avatar?: string | null;
	email: string;
	content: string;
	created_at: string;
	updated_at: string;
};

export function createChallengeServiceApi(http: FloatCTFHttpClient) {
	return {
		fetch: async (
			params: QueryParams = {},
		): Promise<UniResponse<ChallengesListItem[]>> => {
			const res = await http.get("/challenges", { params });
			return res.data;
		},
		get: async (id: string): Promise<UniResponse<ChallengesListItem>> => {
			const res = await http.get(`/challenges/${id}`);
			return res.data;
		},
		getInstance: async (id: string): Promise<UniResponse<Instances>> => {
			const res = await http.get(`/challenges/${id}/instance`);
			return res.data;
		},
		getMyWriteup: async (
			challenge_id: string,
		): Promise<UniResponse<ChallengeWriteup>> => {
			const res = await http.get(`/challenges/${challenge_id}/my_writeup`);
			return res.data;
		},
		createMyWriteup: async ({
			challenge_id,
			content,
		}: {
			challenge_id: string;
			content: string;
		}): Promise<UniResponse<ChallengeWriteup>> => {
			const res = await http.post(
				`/challenges/${challenge_id}/my_writeup`,
				{
					content,
				},
			);
			return res.data;
		},
		getWriteup: async (
			id: string,
		): Promise<UniResponse<UnifiedWriteupDetail>> => {
			const res = await http.get(`/writeups/${id}`);
			return res.data;
		},
		getWriteups: async (
			challenge_id: string,
		): Promise<UniResponse<ChallengeWriteupResult[]>> => {
			const res = await http.get(`/challenges/${challenge_id}/writeups`);
			return res.data;
		},
		getAllWriteups: async (
			params: QueryParams = {},
		): Promise<UniResponse<UnifiedWriteupResult[]>> => {
			const res = await http.get("/writeups", { params });
			return res.data;
		},
		getChallengeSets: async (): Promise<UniResponse<ChallengeSets[]>> => {
			const res = await http.get("/challenge_sets");
			return res.data;
		},
		getChallengeSet: async (
			id: string,
		): Promise<UniResponse<ChallengesListItem[]>> => {
			const res = await http.get(`/challenge_sets/${id}`);
			return res.data;
		},
};
}

export type ChallengeServiceApi = ReturnType<typeof createChallengeServiceApi>;
