import type { SuperAdmin } from "../../entity/index.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";

export function createSuperAdminApi(http: FloatCTFHttpClient) {
	return {
	    fetch: async (
	        params: QueryParams = {},
	    ): Promise<UniResponse<SuperAdmin[]>> => {
	        const res = await http.get("/super_admin", { params });
	        return res.data;
	    },
	    create: async (
	        data: Partial<SuperAdmin>,
	    ): Promise<UniResponse<SuperAdmin>> => {
	        const res = await http.post("/super_admin", data);
	        return res.data;
	    },
	    remove: async (id_list: string[]): Promise<UniResponse<number>> => {
	        const res = await http.delete("/super_admin", {
	            data: { id_list },
	        });
	        return res.data;
	    },
	    patch: async (
	        id: string,
	        data: Partial<SuperAdmin>,
	    ): Promise<UniResponse<SuperAdmin>> => {
	        const res = await http.post(`/super_admin/${id}`, data);
	        return res.data;
	    },
};
}

export type SuperAdminApi = ReturnType<typeof createSuperAdminApi>;
