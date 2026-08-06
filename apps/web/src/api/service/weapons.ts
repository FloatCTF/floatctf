import type { Weapons } from "@/entity/weapons";
import { type QueryParams, type UniResponse, service_api } from "../axios";

export const weaponsServiceApi = {
    fetch: async (
        params: QueryParams = {},
    ): Promise<UniResponse<Weapons[]>> => {
        const res = await service_api.get("/weapons", { params });
        return res.data;
    },
};
