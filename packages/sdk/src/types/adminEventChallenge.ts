import type { Challenges } from "../entity/index.js";

/** 管理端赛事挂题行（jeopardy_event_challenges）。 */
export type EventChallenge = {
	event_id: string;
	challenge_id: string;
	hidden: boolean;
	points: number;
};

/** 管理端赛事题目列表条目（挂题行 + 题目实体）。 */
export type EventChallengeResult = {
	event_challenge: EventChallenge;
	challenge: Challenges;
};
