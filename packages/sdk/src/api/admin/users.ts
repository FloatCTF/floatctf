import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";
import type { UserDto, UserWriteRequest } from "../../types/account.js";
import type { UserRolesDto } from "../../types/roles.js";

export function createUserAdminApi(http: FloatCTFHttpClient) {
	return {
	    fetch: async (params: QueryParams = {}): Promise<UniResponse<UserDto[]>> => {
	        // 注意：**不要**在这里打印 res.data —— 历史实现有一处
	        // `console.log(res.data)`，会把（当时的）整份账号列表写进浏览器控制台。
	        const res = await http.get("/users", { params });
	        return res.data;
	    },
	    create: async (user: UserWriteRequest): Promise<UniResponse<UserDto>> => {
	        const res = await http.post("/users", user);
	        return res.data;
	    },
	    patch: async (user: UserWriteRequest): Promise<UniResponse<UserDto>> => {
	        // 与 `GenericTable.patchFn(data: Partial<T>)` 兼容：`Partial<T>` 可直接赋给
	        // `UserWriteRequest`。id 由调用方（表格已合并行数据）保证存在，这里做一次
	        // 运行时防御，避免拼出 `/users/undefined`。
	        if (!user.id) {
	            throw new Error("admin.users.patch requires an id");
	        }
	        const res = await http.patch(`/users/${user.id}`, user);
	        return res.data;
	    },
	    remove: async (id_list: string[]): Promise<UniResponse<number>> => {
	        const res = await http.delete("/users", { data: { id_list } });
	        return res.data;
	    },
	    /** 某账号的角色与有效权限（`users.read`）。 */
	    getRoles: async (userId: string): Promise<UniResponse<UserRolesDto>> => {
	        const res = await http.get(`/users/${userId}/roles`);
	        return res.data;
	    },
	    /**
	     * **整体替换**某账号的角色集合（`roles.manage`）。
	     *
	     * 不是增量：增量语义下「先加后删」与「先删后加」结果不同，
	     * 会造出短暂的权限空洞或超额授权窗口。
	     */
	    setRoles: async (
	        userId: string,
	        roleIds: string[],
	    ): Promise<UniResponse<UserRolesDto>> => {
	        const res = await http.put(`/users/${userId}/roles`, {
	            role_ids: roleIds,
	        });
	        return res.data;
	    },
};
}

export type UserAdminApi = ReturnType<typeof createUserAdminApi>;
