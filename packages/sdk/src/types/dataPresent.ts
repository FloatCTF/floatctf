import type { Events } from "../entity/index.js";
import type { ScoreboardItem } from "./scoreboard.js";
import type { TrendItem } from "./trend.js";

/** 数据大屏：单题聚合。 */
export type DataEventChallenge = {
	name: string;
	category: string;
	points: number;
	solved_count: number;
	solved_percent: number;
};

/** 数据大屏：最近解出流水。 */
export type DataEventChallengeSolve = {
	user_nickname: string;
	challenge_name: string;
	challenge_category: string;
	created_at: string; // NaiveDateTime → string
	bonus_points: number;
};

/** 数据大屏聚合响应。 */
export type DataPresent = {
	event: Events; // 对应 events::Model
	user_count: number;
	team_count: number;
	solved_recent_15: DataEventChallengeSolve[];
	event_challenges: DataEventChallenge[];
	scoreboard_top10: ScoreboardItem[];
	trend: TrendItem[];
};
