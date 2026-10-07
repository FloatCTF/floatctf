import type { EventUserResult } from "../../types/adminEventUser.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";

export function createEventUserAdminApi(http: FloatCTFHttpClient) {
	return {
	    fetch: (event_id: string) => {
	        return async (
	            params: QueryParams = {},
	        ): Promise<UniResponse<EventUserResult[]>> => {
	            const res = await http.get(`/events/${event_id}/users`, {
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
	        return http.post(`/events/${event_id}/users`, {
	            user_id,
	            user_id_list,
	        });
	    },
	    delete: (event_id: string) => {
	        return async (id_list: string[]): Promise<UniResponse<number>> => {
	            const res = await http.delete(`/events/${event_id}/users`, {
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
	        const res = await http.post(
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
	        const res = await http.post(
	            `/events/${event_id}/users/${user_id}/unbanned`,
	        );
	        return res.data;
	    },
};
}

export type EventUserAdminApi = ReturnType<typeof createEventUserAdminApi>;
