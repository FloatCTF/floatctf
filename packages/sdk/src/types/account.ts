/**
 * 账号相关 DTO。
 *
 * ## 安全不变量（#1）：password 永不出现在响应类型里
 *
 * 生成实体（`@floatctf/sdk/entity` 的 `Users` / `SuperAdmin`）**是 DB 列镜像**，
 * 其中含 `password` 列——它们**不得**被当作 API 响应类型复用。
 * 历史缺陷：管理端用户列表与超管列表直接复用生成实体作为响应类型，
 * 导致 argon2id 哈希进入前端类型与运行时数据。
 *
 * 因此这里独立定义**响应 DTO**（无 password）与**写入请求 DTO**（password 只在请求）。
 */

/**
 * 账号响应体。
 *
 * 与后端 `apps/api/src/modules/identity/user/dto.rs::UsersDto` 对应，
 * 刻意**不含** `password` / `password_hash`。
 */
export type UserDto = {
	avatar: string | null;
	created_at: string;
	email: string;
	id: string;
	nickname: string;
	updated_at: string;
	username: string;
};

/**
 * `GET /api/users/me` 的响应体：账号字段 + 有效权限。
 *
 * `permissions` 是**展示用**信息（用于隐藏/显示导航入口），**不是**授权依据：
 * 真正的判定永远在服务端每个请求上。前端不应因为 `permissions` 里有某项
 * 就跳过错误处理 —— 权限随时可能被撤销（后端每请求重解析）。
 */
export type MeDto = UserDto & {
	/** 有效权限的稳定字符串（多角色并集；全权角色下为全部权限）。 */
	permissions: string[];
	/** 是否持有全权角色（内置 `super-admin`）。 */
	grants_all: boolean;
};

/**
 * 管理员账号响应体（**临时**，Wave 10 删除 `super_admin` 端点后一并移除）。
 *
 * 与后端 `administrator/dto.rs::SuperAdminDto` 对应，刻意**不含** `password`。
 */
export type SuperAdminDto = {
	created_at: string;
	email: string;
	id: string;
	updated_at: string;
	username: string;
};

/**
 * 建号 / 改号请求体。
 *
 * `password` **只允许出现在请求里**；响应永远不回显它。
 */
export type UserWriteRequest = Partial<UserDto> & {
	password?: string;
};

/** 管理员建号 / 改号请求体（同样是请求专用）。 */
export type SuperAdminWriteRequest = Partial<SuperAdminDto> & {
	password?: string;
};
