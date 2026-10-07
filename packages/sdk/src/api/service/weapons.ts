import type { Weapons } from "../../entity/weapons.js";
import { type QueryParams, type UniResponse, service_api } from "../../transport.js";

export const weaponsServiceApi = {
    fetch: async (
        params: QueryParams = {},
    ): Promise<UniResponse<Weapons[]>> => {
        const res = await service_api.get("/weapons", { params });
        return res.data;
    },
};
