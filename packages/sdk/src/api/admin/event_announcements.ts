import type { EventAnnouncements } from "../../entity/index.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";

export function createEventAnnouncementAdminApi(http: FloatCTFHttpClient) {
	return {
	    fetch: (event_id: string) => {
	        return async (
	            params: QueryParams = {},
	        ): Promise<UniResponse<EventAnnouncements[]>> => {
	            const res = await http.get(
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
	            const res = await http.post(
	                `/events/${event_id}/announcements`,
	                announcement,
	            );
	            return res.data;
	        };
	    },
	    patch: (event_id: string) => {
	        return async (announcement: Partial<EventAnnouncements>) => {
	            const res = await http.patch(
	                `/events/${event_id}/announcements/${announcement.id}`,
	                announcement,
	            );
	            return res.data;
	        };
	    },
	    remove: (event_id: string) => {
	        return async (id_list: string[]) => {
	            const res = await http.delete(
	                `/events/${event_id}/announcements`,
	                {
	                    data: { id_list },
	                },
	            );
	            return res.data;
	        };
	    },
};
}

export type EventAnnouncementAdminApi = ReturnType<typeof createEventAnnouncementAdminApi>;
