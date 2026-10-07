import type { ScheduledTasks } from "../../entity/index.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";

export function createScheduledTaskAdminApi(http: FloatCTFHttpClient) {
	return {
	    fetch: async (
	        params: QueryParams = {},
	    ): Promise<UniResponse<ScheduledTasks[]>> => {
	        const res = await http.get("/scheduled_tasks", { params });
	        return res.data;
	    },
	    create: async (
	        task: Partial<ScheduledTasks>,
	    ): Promise<UniResponse<ScheduledTasks>> => {
	        const res = await http.post("/scheduled_tasks", task);
	        return res.data;
	    },
	    patch: async (
	        task: Partial<ScheduledTasks>,
	    ): Promise<UniResponse<ScheduledTasks>> => {
	        const res = await http.patch(`/scheduled_tasks/${task.id}`, task);
	        return res.data;
	    },
	    remove: async (id_list: string[]): Promise<UniResponse<number>> => {
	        const res = await http.delete("/scheduled_tasks", {
	            data: { id_list },
	        });
	        return res.data;
	    },
	    run: async (task_id: string): Promise<UniResponse<ScheduledTasks>> => {
	        const res = await http.post(`/scheduled_tasks/${task_id}/run`);
	        return res.data;
	    },
};
}

export type ScheduledTaskAdminApi = ReturnType<typeof createScheduledTaskAdminApi>;
