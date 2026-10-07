import type { ChallengesListItem } from "./challengeDto.js";

/** 选手端赛事题目列表条目。 */
export type EventChallengeResult = {
	/** 行唯一标识（后端不返回，前端由 challenge.id 合成，供 DataTable 行 key）。 */
	id: string;
	challenge: ChallengesListItem;
	current_points: number;
	solved_count: number;
	solved: boolean;
	solved_no: number;
};
