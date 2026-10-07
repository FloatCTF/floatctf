import type { EventLogs } from "../../entity/index.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";

export function createEventLogAdminApi(http: FloatCTFHttpClient) {
	return {
	    fetch: (event_id: string) => {
	        return async (
	            params: QueryParams = {},
	        ): Promise<UniResponse<EventLogs[]>> => {
	            const res = await http.get(`/events/${event_id}/logs`, {
	                params,
	            });
	            return res.data;
	        };
	    },
};
}

export type EventLogAdminApi = ReturnType<typeof createEventLogAdminApi>;
