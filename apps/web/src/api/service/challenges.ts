import type {
    ChallengeSets,
    ChallengeWriteup,
    Challenges,
    Instances,
} from "@/entity";
import type { ChallengeWriteupResult } from "@/routes/service/challenges/$id/writeup";
import { type QueryParams, type UniResponse, service_api } from "../axios";

export const challengeServiceApi = {
    fetch: async (
        params: QueryParams = {},
    ): Promise<UniResponse<Challenges[]>> => {
        const res = await service_api.get("/challenges", { params });
        return res.data;
    },
    get: async (id: string): Promise<UniResponse<Challenges>> => {
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
        const res = await service_api.get(
            `/challenges/${challenge_id}/my_writeup`,
        );
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
    ): Promise<UniResponse<ChallengeWriteupResult>> => {
        const res = await service_api.get(`/writeups/${id}`);
        return res.data;
    },
    getWriteups: async (
        challenge_id: string,
    ): Promise<UniResponse<ChallengeWriteupResult[]>> => {
        const res = await service_api.get(
            `/challenges/${challenge_id}/writeups`,
        );
        return res.data;
    },
    getAllWriteups: async (
        params: QueryParams = {},
    ): Promise<UniResponse<ChallengeWriteupResult[]>> => {
        const res = await service_api.get("/writeups", { params });
        return res.data;
    },
    getChallengeSets: async (): Promise<UniResponse<ChallengeSets[]>> => {
        const res = await service_api.get("/challenge_sets");
        return res.data;
    },
    getChallengeSet: async (id: string): Promise<UniResponse<Challenges[]>> => {
        const res = await service_api.get(`/challenge_sets/${id}`);
        return res.data;
    },
};
