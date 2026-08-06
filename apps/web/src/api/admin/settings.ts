import type { Settings } from "@/entity";
import { type UniResponse, admin_api } from "@/api/axios";

export const settingAdminApi = {
    fetch: async (): Promise<UniResponse<Settings[]>> => {
        const res = await admin_api.get("/settings");
        return res.data;
    },
    create: async (
        setting: Partial<Settings>,
    ): Promise<UniResponse<Settings>> => {
        const res = await admin_api.post("/settings", setting);
        return res.data;
    },
    remove: async (id_list: string[]): Promise<UniResponse<number>> => {
        const res = await admin_api.delete("/settings", { data: { id_list } });
        return res.data;
    },
    patch: async (
        setting: Partial<Settings>,
    ): Promise<UniResponse<Settings>> => {
        const res = await admin_api.patch(`/settings/${setting.id}`, setting);
        return res.data;
    },
};
