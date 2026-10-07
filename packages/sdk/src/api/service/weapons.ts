import type { Weapons } from "../../entity/weapons.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";

export function createWeaponsServiceApi(http: FloatCTFHttpClient) {
	return {
	    fetch: async (
	        params: QueryParams = {},
	    ): Promise<UniResponse<Weapons[]>> => {
	        const res = await http.get("/weapons", { params });
	        return res.data;
	    },
};
}

export type WeaponsServiceApi = ReturnType<typeof createWeaponsServiceApi>;
