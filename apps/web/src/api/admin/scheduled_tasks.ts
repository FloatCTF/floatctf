import type { ScheduledTasks } from "@/entity";
import { type QueryParams, type UniResponse, admin_api } from "@/api/axios";

export const scheduledTaskAdminApi = {
    fetch: async (
        params: QueryParams = {},
    ): Promise<UniResponse<ScheduledTasks[]>> => {
        const res = await admin_api.get("/scheduled_tasks", { params });
        return res.data;
    },
    create: async (
        task: Partial<ScheduledTasks>,
    ): Promise<UniResponse<ScheduledTasks>> => {
        const res = await admin_api.post("/scheduled_tasks", task);
        return res.data;
    },
    patch: async (
        task: Partial<ScheduledTasks>,
    ): Promise<UniResponse<ScheduledTasks>> => {
        const res = await admin_api.patch(`/scheduled_tasks/${task.id}`, task);
        return res.data;
    },
    remove: async (id_list: string[]): Promise<UniResponse<number>> => {
        const res = await admin_api.delete("/scheduled_tasks", {
            data: { id_list },
        });
        return res.data;
    },
    run: async (task_id: string): Promise<UniResponse<ScheduledTasks>> => {
        const res = await admin_api.post(`/scheduled_tasks/${task_id}/run`);
        return res.data;
    },
};
