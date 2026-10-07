/** 赛事走势图数据点（时间 + 当时总分）。 */
export type TrendPoint = {
	name: string;
	score: number; // total score
	time: string; // NaiveDateTime → ISO 字符串
};

/** 走势图单个主体（个人或战队）的折线。 */
export type TrendItem = {
	name: string;
	points: TrendPoint[];
};
