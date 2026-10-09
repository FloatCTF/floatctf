import type { FloatCTFHttpClient } from "../../transport.js";
import type { UniResponse } from "../../transport.js";
import type {
	CreateRoleRequest,
	PermissionDto,
	RoleDto,
	UpdateRoleRequest,
} from "../../types/roles.js";

/**
 * 管理端角色 / 权限 API（v2 新增）。
 *
 * 后端对每个端点单独做权限判定（`roles.read` / `roles.manage`）并施加反提权规则：
 * 你**不能**授予或保留自己没有的权限，也不能修改全权账号的角色集合。
 * 因此这里不需要（也不应该）在前端重复实现反提权判断 ——
 * 前端只负责把 403 的文案如实展示出来。
 */
export function createRoleAdminApi(http: FloatCTFHttpClient) {
	return {
		/** 权限目录（29 条，含域与高危标记）。 */
		listPermissions: async (): Promise<UniResponse<PermissionDto[]>> => {
			const res = await http.get("/permissions");
			return res.data;
		},
		/** 域列表（界面分组用）。 */
		listPermissionDomains: async (): Promise<UniResponse<string[]>> => {
			const res = await http.get("/permissions/domains");
			return res.data;
		},
		/** 角色列表（含权限集与持有账号数）。 */
		fetch: async (): Promise<UniResponse<RoleDto[]>> => {
			const res = await http.get("/roles");
			return res.data;
		},
		get: async (roleId: string): Promise<UniResponse<RoleDto>> => {
			const res = await http.get(`/roles/${roleId}`);
			return res.data;
		},
		create: async (body: CreateRoleRequest): Promise<UniResponse<RoleDto>> => {
			const res = await http.post("/roles", body);
			return res.data;
		},
		patch: async (
			roleId: string,
			body: UpdateRoleRequest,
		): Promise<UniResponse<RoleDto>> => {
			const res = await http.patch(`/roles/${roleId}`, body);
			return res.data;
		},
		/**
		 * 删除角色。
		 *
		 * 内置角色（`protected`）返回 **409**（不是 403）：请求本身没问题，
		 * 是它与该资源的内置不变式冲突。
		 */
		remove: async (roleId: string): Promise<UniResponse<null>> => {
			const res = await http.delete(`/roles/${roleId}`);
			return res.data;
		},
	};
}

export type RoleAdminApi = ReturnType<typeof createRoleAdminApi>;
