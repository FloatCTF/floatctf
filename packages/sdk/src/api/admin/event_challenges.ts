import type { JeopardyEventChallenges as EventChallenges } from "../../entity/index.js";
import type { EventChallengeResult } from "../../types/adminEventChallenge.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";

export function createEventChallengeAdminApi(http: FloatCTFHttpClient) {
	return {
	    fetch: (event_id: string) => {
	        return async (
	            params: QueryParams = {},
	        ): Promise<UniResponse<EventChallengeResult[]>> => {
	            const res = await http.get(`/events/${event_id}/challenges`, {
	                params,
	            });
	            return res.data;
	        };
	    },

	    add: async ({
	        event_id,
	        challenge_id_list,
	        challenge_id,
	        points,
	    }: {
	        event_id: string;
	        challenge_id_list?: string[];
	        challenge_id?: string;
	        points?: number;
	    }): Promise<UniResponse<EventChallenges[]>> => {
	        const res = await http.post(`/events/${event_id}/challenges`, {
	            challenge_id_list,
	            challenge_id,
	            points,
	        });
	        return res.data;
	    },
	    setPoints: async ({
	        event_id,
	        challenge_id_list,
	        points,
	    }: {
	        event_id: string;
	        challenge_id_list: string[];
	        points: number;
	    }): Promise<UniResponse<EventChallenges[]>> => {
	        const res = await http.patch(`/events/${event_id}/challenges`, {
	            challenge_id_list,
	            points,
	        });
	        return res.data;
	    },
	    remove: (event_id: string) => {
	        return async (id_list: string[]): Promise<UniResponse<number>> => {
	            console.log(id_list);
	            const res = await http.delete(
	                `/events/${event_id}/challenges`,
	                {
	                    data: { id_list },
	                },
	            );
	            return res.data;
	        };
	    },
	    open: async ({
	        event_id,
	        challenge_id_list,
	        challenge_id,
	    }: {
	        event_id: string;
	        challenge_id_list?: string[];
	        challenge_id?: string;
	    }): Promise<UniResponse<EventChallenges[]>> => {
	        const res = await http.post(
	            `/events/${event_id}/challenges/open`,
	            {
	                challenge_id_list,
	                challenge_id,
	            },
	        );
	        return res.data;
	    },
	    hidden: async ({
	        event_id,
	        challenge_id_list,
	        challenge_id,
	    }: {
	        event_id: string;
	        challenge_id_list?: string[];
	        challenge_id?: string;
	    }): Promise<UniResponse<EventChallenges[]>> => {
	        const res = await http.post(
	            `/events/${event_id}/challenges/hidden`,
	            {
	                challenge_id_list,
	                challenge_id,
	            },
	        );
	        return res.data;
	    },
};
}

export type EventChallengeAdminApi = ReturnType<typeof createEventChallengeAdminApi>;
