import type { SystemInformation } from "@/routes/admin/dashboard";
import { type UniResponse, admin_api } from "@/api/axios";

export const systemAdminApi = {
    monitor: async (): Promise<UniResponse<SystemInformation>> => {
        const response = await admin_api.get("/system/monitor");
        return response.data;
    },
    changelog: async (): Promise<UniResponse<string>> => {
        const response = await admin_api.get("/system/changelog");
        return response.data;
    },
};
