import type { Users } from "@/entity";
import { type QueryParams, type UniResponse, admin_api } from "@/api/axios";

export const userAdminApi = {
    fetch: async (params: QueryParams = {}): Promise<UniResponse<Users[]>> => {
        const res = await admin_api.get("/users", { params });
        console.log(res.data);
        return res.data;
    },
    create: async (user: Partial<Users>): Promise<UniResponse<Users>> => {
        const res = await admin_api.post("/users", user);
        return res.data;
    },
    patch: async (user: Partial<Users>): Promise<UniResponse<Users>> => {
        const res = await admin_api.patch(`/users/${user.id}`, user);
        return res.data;
    },
    remove: async (id_list: string[]): Promise<UniResponse<number>> => {
        const res = await admin_api.delete("/users", { data: { id_list } });
        return res.data;
    },
};
