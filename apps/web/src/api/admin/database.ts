import type { SqlResult, SqlStatement } from "@/routes/admin/database";
import { type UniResponse, admin_api } from "@/api/axios";

export const databaseAdminApi = {
    exec_sql: async ({
        sql,
    }: SqlStatement): Promise<UniResponse<SqlResult>> => {
        const res = await admin_api.post("/database/exec_sql", {
            sql,
        });

        return res.data;
    },
};
