import type { Users } from "../../entity/index.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";

export function createUserAdminApi(http: FloatCTFHttpClient) {
	return {
	    fetch: async (params: QueryParams = {}): Promise<UniResponse<Users[]>> => {
	        const res = await http.get("/users", { params });
	        console.log(res.data);
	        return res.data;
	    },
	    create: async (user: Partial<Users>): Promise<UniResponse<Users>> => {
	        const res = await http.post("/users", user);
	        return res.data;
	    },
	    patch: async (user: Partial<Users>): Promise<UniResponse<Users>> => {
	        const res = await http.patch(`/users/${user.id}`, user);
	        return res.data;
	    },
	    remove: async (id_list: string[]): Promise<UniResponse<number>> => {
	        const res = await http.delete("/users", { data: { id_list } });
	        return res.data;
	    },
};
}

export type UserAdminApi = ReturnType<typeof createUserAdminApi>;
