/** 计分榜格子：某主体在某题上的解出状态（后端 `jeopardy::domain::scoreboard::ChallengeScoreboard`）。 */
export type ChallengeScoreboard = {
	name: string;
	solved: boolean;
	solved_no: number;
};

/** 计分榜一行（个人或战队）。 */
export type ScoreboardItem = {
	id: string;
	no: number;
	name: string;
	avatar?: string;
	score: number;
	solved_count: number;
	challenges: ChallengeScoreboard[];
};
