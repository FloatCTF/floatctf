import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";
import type { ChallengeSets } from "../../entity/index.js";
import type {
	BuildChallengeResult,
	ChallengeCheckResult,
	ChallengeScanItem,
	ImportChallengeResponse,
} from "../../types/adminChallenge.js";
import type { ChallengesListItem } from "../../types/challengeDto.js";

export function createChallengeAdminApi(http: FloatCTFHttpClient) {
	return {
		fetch: async (
			params: QueryParams = {},
		): Promise<UniResponse<ChallengesListItem[]>> => {
			const res = await http.get("/challenges", { params });
			return res.data;
		},
		create: async (
			challenge: Partial<ChallengesListItem>,
		): Promise<UniResponse<ChallengesListItem>> => {
			const res = await http.post("/challenges", challenge);
			return res.data;
		},
		patch: async (
			challenge: Partial<ChallengesListItem>,
		): Promise<UniResponse<ChallengesListItem>> => {
			const res = await http.patch(`/challenges/${challenge.id}`, challenge);
			return res.data;
		},
		// todo: 批量删除
		remove: async (id_list: string[]): Promise<UniResponse<number>> => {
			const res = await http.delete("/challenges", {
				data: { id_list },
			});
			return res.data;
		},
		// 包 zip：meta.toml + src/** + attachment/**
		importChallenge: async (
			file: File,
		): Promise<UniResponse<ImportChallengeResponse>> => {
			const form = new FormData();
			form.append("package_zip", file, file.name);

			const res = await http.post("/challenges/import", form, {
				headers: {
					"Content-Type": "multipart/form-data",
				},
			});
			return res.data;
		},
		checkChallenges: async (
			challenge_id_list?: string[],
		): Promise<UniResponse<ChallengeCheckResult[]>> => {
			const res = await http.post("/challenges/check", {
				challenge_id_list,
			});
			return res.data;
		},
		buildChallenges: async (
			challenge_id_list?: string[],
		): Promise<UniResponse<BuildChallengeResult[]>> => {
			const res = await http.post("/challenges/build", {
				challenge_id_list,
			});
			return res.data;
		},
		scanChallenges: async (): Promise<UniResponse<ChallengeScanItem[]>> => {
			const res = await http.post("/challenges/scan");
			return res.data;
		},
		getChallengeSets: async (
			params: QueryParams = {},
		): Promise<UniResponse<ChallengeSets[]>> => {
			const res = await http.get("/challenge_sets", { params });
			return res.data;
		},
		createChallengeSet: async (
			challenge_set: Partial<ChallengeSets>,
		): Promise<UniResponse<ChallengeSets>> => {
			const res = await http.post("/challenge_sets", challenge_set);
			return res.data;
		},
		deleteChallengeSet: async (
			id_list: string[],
		): Promise<UniResponse<number>> => {
			const res = await http.delete("/challenge_sets", {
				data: { id_list },
			});
			return res.data;
		},
		getChallengeSet: (id: string) => {
			return async (
				params: QueryParams = {},
			): Promise<UniResponse<ChallengesListItem[]>> => {
				const res = await http.get(`/challenge_sets/${id}`, {
					params,
				});
				return res.data;
			};
		},
		removeChallengeFromSet: (id: string) => {
			return async (id_list: string[]): Promise<UniResponse<number>> => {
				const res = await http.delete(`/challenge_sets/${id}/challenges`, {
					data: { id_list },
				});
				return res.data;
			};
		},
		addChallengeToSet: async ({
			set_id,
			challenge_id_list,
		}: {
			set_id: string;
			challenge_id_list?: string[];
		}): Promise<UniResponse<null>> => {
			const res = await http.post(`/challenge_sets/${set_id}/challenges`, {
				challenge_id_list,
			});
			return res.data;
		},
		patchChallengeSet: async (
			challenge_set: Partial<ChallengeSets>,
		): Promise<UniResponse<ChallengeSets>> => {
			const res = await http.patch(
				`/challenge_sets/${challenge_set.id}`,
				challenge_set,
			);
			return res.data;
		},
};
}

export type ChallengeAdminApi = ReturnType<typeof createChallengeAdminApi>;
