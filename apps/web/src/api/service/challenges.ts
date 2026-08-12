import type { InstancesDto as Instances } from "./instances";
import type {
	ChallengeSets,
	ChallengeWriteup,
	Challenges,
} from "@/entity";
import type { ChallengeWriteupResult } from "@/routes/service/challenges/$id/writeup";
import type { ChallengesListItem } from "@/types/challengeDto";
import { type QueryParams, type UniResponse, service_api } from "../axios";

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

export const challengeServiceApi = {
	fetch: async (
		params: QueryParams = {},
	): Promise<UniResponse<ChallengesListItem[]>> => {
		const res = await service_api.get("/challenges", { params });
		return res.data;
	},
	get: async (id: string): Promise<UniResponse<ChallengesListItem>> => {
		const res = await service_api.get(`/challenges/${id}`);
		return res.data;
	},
	getInstance: async (id: string): Promise<UniResponse<Instances>> => {
		const res = await service_api.get(`/challenges/${id}/instance`);
		return res.data;
	},
	getMyWriteup: async (
		challenge_id: string,
	): Promise<UniResponse<ChallengeWriteup>> => {
		const res = await service_api.get(`/challenges/${challenge_id}/my_writeup`);
		return res.data;
	},
	createMyWriteup: async ({
		challenge_id,
		content,
	}: {
		challenge_id: string;
		content: string;
	}): Promise<UniResponse<ChallengeWriteup>> => {
		const res = await service_api.post(
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
		const res = await service_api.get(`/writeups/${id}`);
		return res.data;
	},
	getWriteups: async (
		challenge_id: string,
	): Promise<UniResponse<ChallengeWriteupResult[]>> => {
		const res = await service_api.get(`/challenges/${challenge_id}/writeups`);
		return res.data;
	},
	getAllWriteups: async (
		params: QueryParams = {},
	): Promise<UniResponse<UnifiedWriteupResult[]>> => {
		const res = await service_api.get("/writeups", { params });
		return res.data;
	},
	getChallengeSets: async (): Promise<UniResponse<ChallengeSets[]>> => {
		const res = await service_api.get("/challenge_sets");
		return res.data;
	},
	getChallengeSet: async (
		id: string,
	): Promise<UniResponse<ChallengesListItem[]>> => {
		const res = await service_api.get(`/challenge_sets/${id}`);
		return res.data;
	},
};
