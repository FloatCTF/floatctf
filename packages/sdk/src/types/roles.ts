/**
 * 平台 RBAC 的 API DTO（v2 新增）。
 *
 * 权限字符串的**权威在代码**（`apps/api/src/core/permission.rs` 的 29 条），
 * 后端通过 `GET /api/admin/permissions` 把目录暴露出来；
 * 这里不做任何本地权限表，避免出现「第二真相源」。
 */

/** 一条平台权限。 */
export type PermissionDto = {
	/** 稳定字符串，如 `events.manage`。落库 / 审计 / 判定都用它。 */
	key: string;
	/** 所属域（仅用于界面分组）。 */
	domain: string;
	description: string;
	/** 高危权限：界面需要二次确认与醒目提示。 */
	high_risk: boolean;
};

/** 一个平台角色。 */
export type RoleDto = {
	id: string;
	/** 稳定标识；创建后不可改。 */
	key: string;
	name: string;
	description: string;
	/** 系统内置：不可删除，且权限集不可修改。 */
	protected: boolean;
	/**
	 * 全权标记：拥有**全部**权限（含将来新增的）。
	 *
	 * 为 `true` 时 `permissions` 通常为空 —— 它的权限来自短路，
	 * 不是一行行列出来的。界面不要因为「权限为空」就以为它没权限。
	 */
	grants_all: boolean;
	permissions: string[];
	/** 被授予该角色的账号数（删除前的影响面提示）。 */
	user_count: number;
};

export type CreateRoleRequest = {
	key: string;
	name: string;
	description?: string;
	permissions: string[];
};

export type UpdateRoleRequest = {
	name?: string;
	description?: string;
	/** `undefined` = 不改权限集；数组 = **整体替换**。 */
	permissions?: string[];
};

export type RoleSummaryDto = {
	id: string;
	key: string;
	name: string;
};

/** 某账号的角色与有效权限。 */
export type UserRolesDto = {
	user_id: string;
	username: string;
	roles: RoleSummaryDto[];
	/** 有效权限（多角色并集；全权角色下为全集）。 */
	permissions: string[];
};
