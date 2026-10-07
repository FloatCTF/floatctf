import type { EventLogs } from "../../entity/index.js";
import { type QueryParams, type UniResponse, admin_api } from "../../transport.js";

export const eventLogAdminApi = {
    fetch: (event_id: string) => {
        return async (
            params: QueryParams = {},
        ): Promise<UniResponse<EventLogs[]>> => {
            const res = await admin_api.get(`/events/${event_id}/logs`, {
                params,
            });
            return res.data;
        };
    },
};
