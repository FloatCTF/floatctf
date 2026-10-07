import type { EventTeamMemberRole, EventTeams } from "../entity/index.js";

/** 管理端战队成员条目。 */
export type TeamMemberResult = {
	username: string;
	nickname: string;
	role: EventTeamMemberRole;
	points: number;
};

/** 管理端战队列表条目。 */
export type TeamResult = {
	id: string;
	team: EventTeams;
	captain: string;
	members: TeamMemberResult[];
};
