import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";
import type { SuperAdminDto, SuperAdminWriteRequest } from "../../types/account.js";

/**
 * 管理员账号 API —— **临时**（Wave 10 删除）。
 *
 * 统一 Account + RBAC 后，管理员就是持有 `super-admin` 角色的普通 `users` 账号，
 * 因此由 `client.admin.users` + `client.admin.roles` 取代本模块。
 * 在删除前，本模块同样遵守安全不变量 #1：响应类型**不含** `password`。
 */
export function createSuperAdminApi(http: FloatCTFHttpClient) {
	return {
	    fetch: async (
	        params: QueryParams = {},
	    ): Promise<UniResponse<SuperAdminDto[]>> => {
	        const res = await http.get("/super_admin", { params });
	        return res.data;
	    },
	    create: async (
	        data: SuperAdminWriteRequest,
	    ): Promise<UniResponse<SuperAdminDto>> => {
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
	        data: SuperAdminWriteRequest,
	    ): Promise<UniResponse<SuperAdminDto>> => {
	        const res = await http.post(`/super_admin/${id}`, data);
	        return res.data;
	    },
};
}

export type SuperAdminApi = ReturnType<typeof createSuperAdminApi>;
