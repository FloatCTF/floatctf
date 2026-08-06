import type { Weapons } from "@/entity/weapons";
import { type QueryParams, type UniResponse, admin_api } from "@/api/axios";

export const weaponsAdminApi = {
    fetch: async (
        params: QueryParams = {},
    ): Promise<UniResponse<Weapons[]>> => {
        const res = await admin_api.get("/weapons", { params });
        return res.data;
    },
    create: async (weapon: Partial<Weapons>): Promise<UniResponse<Weapons>> => {
        const res = await admin_api.post("/weapons", weapon);
        return res.data;
    },
    patch: async (weapon: Partial<Weapons>): Promise<UniResponse<Weapons>> => {
        const res = await admin_api.patch(`/weapons/${weapon.id}`, weapon);
        return res.data;
    },
    remove: async (id_list: string[]): Promise<UniResponse<number>> => {
        const res = await admin_api.delete("/weapons", { data: { id_list } });
        return res.data;
    },
    upload: async (
        weapon_id: string,
        weapon: File,
    ): Promise<UniResponse<null>> => {
        const formData = new FormData();
        formData.append("weapon", weapon);

        const res = await admin_api.post(
            `/weapons/${weapon_id}/upload`,
            formData,
            {
                headers: {
                    "Content-Type": "multipart/form-data",
                },
            },
        );
        return res.data;
    },
};
