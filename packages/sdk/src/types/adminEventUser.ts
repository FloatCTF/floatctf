import type { EventUsers, Users } from "../entity/index.js";

/** 管理端赛事用户列表条目。 */
export type EventUserResult = {
	id: string;
	user: Users;
	event_user: EventUsers;
};
