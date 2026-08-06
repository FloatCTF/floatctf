import type { ChallengeSets, Challenges } from "@/entity";
import type {
    BuildChallengeResult,
    ChallengeCheckResult,
} from "@/routes/admin/challenges";
import { type QueryParams, type UniResponse, admin_api } from "@/api/axios";

export const challengeAdminApi = {
    fetch: async (
        params: QueryParams = {},
    ): Promise<UniResponse<Challenges[]>> => {
        const res = await admin_api.get("/challenges", { params });
        return res.data;
    },
    create: async (
        challenge: Partial<Challenges>,
    ): Promise<UniResponse<Challenges>> => {
        const res = await admin_api.post("/challenges", challenge);
        return res.data;
    },
    patch: async (
        challenge: Partial<Challenges>,
    ): Promise<UniResponse<Challenges>> => {
        const res = await admin_api.patch(
            `/challenges/${challenge.id}`,
            challenge,
        );
        return res.data;
    },
    // todo: 批量删除
    remove: async (id_list: string[]): Promise<UniResponse<number>> => {
        const res = await admin_api.delete("/challenges", {
            data: { id_list },
        });
        return res.data;
    },
    importChallenge: async (
        file: File,
        isBatch: boolean,
    ): Promise<UniResponse<null>> => {
        const form = new FormData();
        const field = isBatch ? "challenge_list_zip" : "challenge_zip";
        form.append(field, file, file.name);

        const res = await admin_api.post("/challenges/import", form, {
            headers: {
                "Content-Type": "multipart/form-data",
            },
        });
        return res.data;
    },
    checkChallenges: async (
        challenge_id_list?: string[],
    ): Promise<UniResponse<ChallengeCheckResult[]>> => {
        const res = await admin_api.post("/challenges/check", {
            challenge_id_list,
        });
        return res.data;
    },
    buildChallenges: async (
        challenge_id_list?: string[],
    ): Promise<UniResponse<BuildChallengeResult[]>> => {
        const res = await admin_api.post("/challenges/build", {
            challenge_id_list,
        });
        return res.data;
    },
    getChallengeSets: async (
        params: QueryParams = {},
    ): Promise<UniResponse<ChallengeSets[]>> => {
        const res = await admin_api.get("/challenge_sets", { params });
        return res.data;
    },
    createChallengeSet: async (
        challenge_set: Partial<ChallengeSets>,
    ): Promise<UniResponse<ChallengeSets>> => {
        const res = await admin_api.post("/challenge_sets", challenge_set);
        return res.data;
    },
    deleteChallengeSet: async (
        id_list: string[],
    ): Promise<UniResponse<number>> => {
        const res = await admin_api.delete("/challenge_sets", {
            data: { id_list },
        });
        return res.data;
    },
    getChallengeSet: (id: string) => {
        return async (
            params: QueryParams = {},
        ): Promise<UniResponse<Challenges[]>> => {
            const res = await admin_api.get(`/challenge_sets/${id}`, {
                params,
            });
            return res.data;
        };
    },
    removeChallengeFromSet: (id: string) => {
        return async (id_list: string[]): Promise<UniResponse<number>> => {
            const res = await admin_api.delete(
                `/challenge_sets/${id}/challenges`,
                {
                    data: { id_list },
                },
            );
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
        const res = await admin_api.post(
            `/challenge_sets/${set_id}/challenges`,
            {
                challenge_id_list,
            },
        );
        return res.data;
    },
    patchChallengeSet: async (
        challenge_set: Partial<ChallengeSets>,
    ): Promise<UniResponse<ChallengeSets>> => {
        const res = await admin_api.patch(
            `/challenge_sets/${challenge_set.id}`,
            challenge_set,
        );
        return res.data;
    },
};
