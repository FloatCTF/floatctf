import type { SqlResult, SqlStatement } from "../../types/database.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { UniResponse } from "../../transport.js";

export function createDatabaseAdminApi(http: FloatCTFHttpClient) {
	return {
	    exec_sql: async ({
	        sql,
	    }: SqlStatement): Promise<UniResponse<SqlResult>> => {
	        const res = await http.post("/database/exec_sql", {
	            sql,
	        });

	        return res.data;
	    },
};
}

export type DatabaseAdminApi = ReturnType<typeof createDatabaseAdminApi>;
