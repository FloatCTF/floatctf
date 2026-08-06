import type { Announcements } from "@/entity";
import { type QueryParams, type UniResponse, admin_api } from "@/api/axios";

export const announcementAdminApi = {
    fetch: async (
        params: QueryParams = {},
    ): Promise<UniResponse<Announcements[]>> => {
        const res = await admin_api.get("/announcements", { params });
        return res.data;
    },
    create: async (
        announcement: Partial<Announcements>,
    ): Promise<UniResponse<Announcements>> => {
        const res = await admin_api.post("/announcements", announcement);
        return res.data;
    },
    remove: async (id_list: string[]): Promise<UniResponse<number>> => {
        const res = await admin_api.delete("/announcements", {
            data: { id_list },
        });
        return res.data;
    },
    patch: async (
        announcement: Partial<Announcements>,
    ): Promise<UniResponse<Announcements>> => {
        const res = await admin_api.patch(
            `/announcements/${announcement.id}`,
            announcement,
        );
        return res.data;
    },
};
