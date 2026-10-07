import type { EventWriteup } from "../../entity/index.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";

export function createEventWriteupAdminApi(http: FloatCTFHttpClient) {
	return {
	    fetch: (event_id: string) => {
	        return async (
	            params: QueryParams = {},
	        ): Promise<UniResponse<EventWriteup[]>> => {
	            const res = await http.get(`/events/${event_id}/writeups`, {
	                params,
	            });
	            return res.data;
	        };
	    },
};
}

export type EventWriteupAdminApi = ReturnType<typeof createEventWriteupAdminApi>;
