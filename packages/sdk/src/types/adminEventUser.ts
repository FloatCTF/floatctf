import type { EventUsers } from "../entity/index.js";
import type { UserDto } from "./account.js";

/**
 * 管理端赛事用户列表条目。
 *
 * ## 安全不变量（#1）：这里**不能**用生成实体
 *
 * 本类型曾经是 `user: Users`（生成实体）。生成实体是 **DB 列镜像**，
 * 含 `password` 列（argon2id 哈希），而后端当时也确实直接把
 * `users::Model` 塞进响应结构体 —— 于是
 * `GET /api/admin/events/{id}/users`（仅需 `events.read`）
 * **把每个参与者的密码哈希回传给了客户端**，并且这个字段还出现在了
 * 前端的类型定义里，等于邀请调用方去读它。
 *
 * 现在两端都改用 `UserDto`：
 * - 后端：`EventUserResult.user` 是 `identity::user::dto::UsersDto`；
 * - SDK：`user` 是 `UserDto`（与 `UsersDto` 对应，无 password）。
 *
 * **绝对不要**把它改回 `Users`。
 */
export type EventUserResult = {
	/** 行标识 = 账号 id（`event_users` 是复合主键，没有单一 id 列）。 */
	id: string;
	user: UserDto;
	event_user: EventUsers;
};
