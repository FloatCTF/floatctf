import type { TeamResult } from "../../types/adminEventTeam.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { UniResponse } from "../../transport.js";

export function createEventTeamAdminApi(http: FloatCTFHttpClient) {
	return {
	    getTeams: (id: string) => {
	        return async (): Promise<UniResponse<TeamResult[]>> => {
	            const res = await http.get(`/events/${id}/teams`);
	            return res.data;
	        };
	    },
	    remove: (id: string) => {
	        return async (id_list: string[]): Promise<UniResponse<number>> => {
	            const res = await http.delete(`/events/${id}/teams`, {
	                data: { id_list },
	            });
	            return res.data;
	        };
	    },
	    banned: async ({
	        event_id,
	        team_id,
	    }: {
	        event_id: string;
	        team_id: string;
	    }) => {
	        const res = await http.post(
	            `/events/${event_id}/teams/${team_id}/banned`,
	        );
	        return res.data;
	    },
	    unbanned: async ({
	        event_id,
	        team_id,
	    }: {
	        event_id: string;
	        team_id: string;
	    }) => {
	        const res = await http.post(
	            `/events/${event_id}/teams/${team_id}/unbanned`,
	        );
	        return res.data;
	    },
};
}

export type EventTeamAdminApi = ReturnType<typeof createEventTeamAdminApi>;
