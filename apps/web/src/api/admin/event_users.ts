import type { EventUserResult } from "@/routes/admin/events/jeopardy.$id/users";
import { type QueryParams, type UniResponse, admin_api } from "@/api/axios";

export const eventUserAdminApi = {
    fetch: (event_id: string) => {
        return async (
            params: QueryParams = {},
        ): Promise<UniResponse<EventUserResult[]>> => {
            const res = await admin_api.get(`/events/${event_id}/users`, {
                params,
            });
            return res.data;
        };
    },
    add: ({
        event_id,
        user_id,
        user_id_list,
    }: {
        event_id: string;
        user_id?: string;
        user_id_list?: string[];
    }): Promise<UniResponse<null>> => {
        return admin_api.post(`/events/${event_id}/users`, {
            user_id,
            user_id_list,
        });
    },
    delete: (event_id: string) => {
        return async (id_list: string[]): Promise<UniResponse<number>> => {
            const res = await admin_api.delete(`/events/${event_id}/users`, {
                data: { id_list },
            });
            return res.data;
        };
    },
    banned: async ({
        event_id,
        user_id,
    }: {
        event_id: string;
        user_id: string;
    }): Promise<UniResponse<EventUserResult>> => {
        const res = await admin_api.post(
            `/events/${event_id}/users/${user_id}/banned`,
        );
        return res.data;
    },
    unbanned: async ({
        event_id,
        user_id,
    }: {
        event_id: string;
        user_id: string;
    }): Promise<UniResponse<EventUserResult>> => {
        const res = await admin_api.post(
            `/events/${event_id}/users/${user_id}/unbanned`,
        );
        return res.data;
    },
};
