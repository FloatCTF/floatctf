import type { Events } from "@/entity";
import type { DataPresent } from "@/routes/admin/events/jeopardy.$id/data_present";
import { type QueryParams, type UniResponse, admin_api } from "@/api/axios";

export const eventAdminApi = {
    fetch: async (params: QueryParams = {}): Promise<UniResponse<Events[]>> => {
        const res = await admin_api.get("/events", { params });
        return res.data;
    },
    create: async (event: Partial<Events>): Promise<UniResponse<Events>> => {
        const res = await admin_api.post("/events", event);
        return res.data;
    },
    patch: async (event: Partial<Events>): Promise<UniResponse<Events>> => {
        const res = await admin_api.patch(`/events/${event.id}`, event);
        return res.data;
    },
    remove: async (id_list: string[]): Promise<UniResponse<number>> => {
        const res = await admin_api.delete("/events", { data: { id_list } });
        return res.data;
    },
    get: async (id: string): Promise<UniResponse<Events>> => {
        const res = await admin_api.get(`/events/${id}`);
        return res.data;
    },
    getData: async (id: string): Promise<UniResponse<DataPresent>> => {
        const res = await admin_api.get(`/events/${id}/data`);
        return res.data;
    },
    getReport: async (event_id: string): Promise<UniResponse<string>> => {
        const res = await admin_api.get(`/events/${event_id}/report`);
        return res.data;
    },
    exportWriteUps: async (event_id: string): Promise<UniResponse<string>> => {
        const res = await admin_api.get(`/events/${event_id}/report`);
        return res.data;
    },
    createChallengeSet: async ({
        name,
        description,
        challenge_id_list,
    }: {
        name: string;
        description?: string;
        challenge_id_list: string[];
    }) => {
        const res = await admin_api.post("/challenge_sets", {
            name,
            description,
            challenge_id_list,
        });
        return res.data;
    },
};
