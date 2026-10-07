import type { EventTeamMembers, EventTeams, Events } from "../entity/index.js";

/** 赛事详情接口返回的成员条目。 */
export type EventTeamMemberResult = {
	member_name: string;
	member: EventTeamMembers;
};

/** 赛事详情接口返回的战队（含成员）。 */
export type EventTeamResult = {
	team: EventTeams;
	members: EventTeamMemberResult[];
};

/** 选手端赛事列表/详情条目。 */
export type EventInfo = {
	id: string;
	event: Events;
	team_result?: EventTeamResult;
	joined: boolean;
};
