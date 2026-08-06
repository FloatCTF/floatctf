import type { EventAnnouncements } from "@/entity";
import { type QueryParams, type UniResponse, admin_api } from "@/api/axios";

export const eventAnnouncementAdminApi = {
    fetch: (event_id: string) => {
        return async (
            params: QueryParams = {},
        ): Promise<UniResponse<EventAnnouncements[]>> => {
            const res = await admin_api.get(
                `/events/${event_id}/announcements`,
                {
                    params,
                },
            );
            return res.data;
        };
    },

    create: (event_id: string) => {
        return async (announcement: Partial<EventAnnouncements>) => {
            const res = await admin_api.post(
                `/events/${event_id}/announcements`,
                announcement,
            );
            return res.data;
        };
    },
    patch: (event_id: string) => {
        return async (announcement: Partial<EventAnnouncements>) => {
            const res = await admin_api.patch(
                `/events/${event_id}/announcements/${announcement.id}`,
                announcement,
            );
            return res.data;
        };
    },
    remove: (event_id: string) => {
        return async (id_list: string[]) => {
            const res = await admin_api.delete(
                `/events/${event_id}/announcements`,
                {
                    data: { id_list },
                },
            );
            return res.data;
        };
    },
};
