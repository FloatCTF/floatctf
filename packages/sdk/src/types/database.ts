/** 管理端 SQL 控制台：单条语句请求。 */
export type SqlStatement = {
	sql: string;
};

/** 管理端 SQL 控制台：执行结果。 */
export type SqlResult = {
	sql_type: string;
	// biome-ignore lint/suspicious/noExplicitAny: 任意列的查询结果
	rows: Record<string, any>[];
	count: number;
	rows_affected: number;
	elapsed_ms: number;
};
