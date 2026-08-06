import type { EventWriteup } from "@/entity";
import { type QueryParams, type UniResponse, admin_api } from "@/api/axios";

export const eventWriteupAdminApi = {
    fetch: (event_id: string) => {
        return async (
            params: QueryParams = {},
        ): Promise<UniResponse<EventWriteup[]>> => {
            const res = await admin_api.get(`/events/${event_id}/writeups`, {
                params,
            });
            return res.data;
        };
    },
};
