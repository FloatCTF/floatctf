import type { Instances } from "@/entity";
import { type QueryParams, type UniResponse, service_api } from "../axios";

export const instanceServiceApi = {
    launch: async (id: string): Promise<UniResponse<Instances>> => {
        const res = await service_api.post("/instances/launch", {
            challenge_id: id,
        });
        return res.data;
    },
    launchSingle: async (
        challenge_id: string,
        event_id: string,
    ): Promise<UniResponse<Instances>> => {
        const res = await service_api.post("/instances/launch", {
            challenge_id,
            event_id,
        });
        return res.data;
    },
    fetch: async (
        params: QueryParams = {},
    ): Promise<UniResponse<Instances[]>> => {
        const res = await service_api.get("/instances", { params });
        return res.data;
    },
    destroy: async (id: string): Promise<UniResponse<number>> => {
        const res = await service_api.delete(`/instances/${id}`);
        return res.data;
    },
};
