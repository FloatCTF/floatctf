import type { Weapons } from "../../entity/weapons.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";

export function createWeaponsAdminApi(http: FloatCTFHttpClient) {
	return {
	    fetch: async (
	        params: QueryParams = {},
	    ): Promise<UniResponse<Weapons[]>> => {
	        const res = await http.get("/weapons", { params });
	        return res.data;
	    },
	    create: async (weapon: Partial<Weapons>): Promise<UniResponse<Weapons>> => {
	        const res = await http.post("/weapons", weapon);
	        return res.data;
	    },
	    patch: async (weapon: Partial<Weapons>): Promise<UniResponse<Weapons>> => {
	        const res = await http.patch(`/weapons/${weapon.id}`, weapon);
	        return res.data;
	    },
	    remove: async (id_list: string[]): Promise<UniResponse<number>> => {
	        const res = await http.delete("/weapons", { data: { id_list } });
	        return res.data;
	    },
	    upload: async (
	        weapon_id: string,
	        weapon: File,
	    ): Promise<UniResponse<null>> => {
	        const formData = new FormData();
	        formData.append("weapon", weapon);

	        const res = await http.post(
	            `/weapons/${weapon_id}/upload`,
	            formData,
	            {
	                headers: {
	                    "Content-Type": "multipart/form-data",
	                },
	            },
	        );
	        return res.data;
	    },
};
}

export type WeaponsAdminApi = ReturnType<typeof createWeaponsAdminApi>;
