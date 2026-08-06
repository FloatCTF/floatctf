import type { EventChallenges } from "@/entity";
import type { EventChallengeResult } from "@/routes/admin/events/jeopardy.$id";
import { type QueryParams, type UniResponse, admin_api } from "@/api/axios";

export const eventChallengeAdminApi = {
    fetch: (event_id: string) => {
        return async (
            params: QueryParams = {},
        ): Promise<UniResponse<EventChallengeResult[]>> => {
            const res = await admin_api.get(`/events/${event_id}/challenges`, {
                params,
            });
            return res.data;
        };
    },

    add: async ({
        event_id,
        challenge_id_list,
        challenge_id,
    }: {
        event_id: string;
        challenge_id_list?: string[];
        challenge_id?: string;
    }): Promise<UniResponse<EventChallenges[]>> => {
        const res = await admin_api.post(`/events/${event_id}/challenges`, {
            challenge_id_list,
            challenge_id,
        });
        return res.data;
    },
    remove: (event_id: string) => {
        return async (id_list: string[]): Promise<UniResponse<number>> => {
            console.log(id_list);
            const res = await admin_api.delete(
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
        const res = await admin_api.post(
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
        const res = await admin_api.post(
            `/events/${event_id}/challenges/hidden`,
            {
                challenge_id_list,
                challenge_id,
            },
        );
        return res.data;
    },
};
