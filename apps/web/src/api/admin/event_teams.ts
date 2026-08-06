import type { TeamResult } from "@/routes/admin/events/jeopardy.$id/teams";
import { type UniResponse, admin_api } from "@/api/axios";

export const eventTeamAdminApi = {
    getTeams: (id: string) => {
        return async (): Promise<UniResponse<TeamResult[]>> => {
            const res = await admin_api.get(`/events/${id}/teams`);
            return res.data;
        };
    },
    remove: (id: string) => {
        return async (id_list: string[]): Promise<UniResponse<number>> => {
            const res = await admin_api.delete(`/events/${id}/teams`, {
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
        const res = await admin_api.post(
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
        const res = await admin_api.post(
            `/events/${event_id}/teams/${team_id}/unbanned`,
        );
        return res.data;
    },
};
