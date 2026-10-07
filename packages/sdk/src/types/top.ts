/** `GET /api/solves/top` 排行榜条目（选手端 Top 15）。 */
export type TopUser = {
	no: number;
	nickname: string;
	avatar?: string;
	solved_count: number;
	solved_last_at: string;
};
