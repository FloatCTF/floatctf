import type { SuperAdmin } from "@/entity";
import { type QueryParams, type UniResponse, admin_api } from "@/api/axios";

export const superAdminApi = {
    fetch: async (
        params: QueryParams = {},
    ): Promise<UniResponse<SuperAdmin[]>> => {
        const res = await admin_api.get("/super_admin", { params });
        return res.data;
    },
    create: async (
        data: Partial<SuperAdmin>,
    ): Promise<UniResponse<SuperAdmin>> => {
        const res = await admin_api.post("/super_admin", data);
        return res.data;
    },
    remove: async (id_list: string[]): Promise<UniResponse<number>> => {
        const res = await admin_api.delete("/super_admin", {
            data: { id_list },
        });
        return res.data;
    },
    patch: async (
        id: string,
        data: Partial<SuperAdmin>,
    ): Promise<UniResponse<SuperAdmin>> => {
        const res = await admin_api.post(`/super_admin/${id}`, data);
        return res.data;
    },
};
