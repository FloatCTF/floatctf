import type {
    EventAnnouncements,
    EventTeams,
    EventUsers,
    Instances,
} from "@/entity";
import type { EventInfo } from "@/routes/service/events";
import type { EventChallengeResult } from "@/routes/service/events/jeopardy.$id/challenges";
import type { EventInstanceResult } from "@/routes/service/events/jeopardy.$id/instances";
import type { ScoreboardItem } from "@/routes/service/events/jeopardy.$id/scoreboard";
import type { TrendItem } from "@/routes/service/events/jeopardy.$id/trend";
import { type QueryParams, type UniResponse, service_api } from "../axios";

export const eventServiceApi = {
    fetch: async (
        params: QueryParams = {},
    ): Promise<UniResponse<EventInfo[]>> => {
        const res = await service_api.get("/events", { params });
        return res.data;
    },
    join: async (event_id: string): Promise<UniResponse<EventUsers>> => {
        const res = await service_api.post(`/events/${event_id}/join`);
        return res.data;
    },
    leave: async (event_id: string): Promise<UniResponse<number>> => {
        const res = await service_api.delete(`/events/${event_id}/leave`);
        return res.data;
    },
    createTeam: async ({
        event_id,
        name,
    }: {
        event_id: string;
        name: string;
    }): Promise<UniResponse<EventTeams>> => {
        const res = await service_api.post(`/events/${event_id}/team`, {
            name,
        });
        return res.data;
    },
    joinTeam: async ({
        event_id,
        team_id,
    }: {
        event_id: string;
        team_id: string;
    }): Promise<UniResponse<null>> => {
        const res = await service_api.post(
            `/events/${event_id}/team/${team_id}/join`,
        );
        return res.data;
    },
    quitTeam: async ({
        event_id,
        team_id,
    }: {
        event_id: string;
        team_id: string;
    }): Promise<UniResponse<null>> => {
        const res = await service_api.delete(
            `/events/${event_id}/team/${team_id}`,
        );
        return res.data;
    },
    get: async (id: string): Promise<UniResponse<EventInfo>> => {
        const res = await service_api.get(`/events/${id}`);
        return res.data;
    },
    fetchChallenges: async (
        id: string,
        params: QueryParams = {},
    ): Promise<UniResponse<EventChallengeResult[]>> => {
        const res = await service_api.get(`/events/${id}/challenges`, {
            params,
        });
        return res.data;
    },
    getChallengeInstance: async (
        event_id: string,
        challenge_id: string,
    ): Promise<UniResponse<Instances>> => {
        const res = await service_api.get(
            `/events/${event_id}/challenges/${challenge_id}/instance`,
        );
        return res.data;
    },
    getInstances: async (
        event_id: string,
    ): Promise<UniResponse<EventInstanceResult[]>> => {
        const res = await service_api.get(`/events/${event_id}/instances`);
        return res.data;
    },
    launchSingleInstance: async (
        event_id: string,
        challenge_id: string,
    ): Promise<UniResponse<Instances>> => {
        const res = await service_api.post("/instances/launch", {
            challenge_id,
            event_id,
        });
        return res.data;
    },
    getScoreboard: async (
        event_id: string,
    ): Promise<UniResponse<ScoreboardItem[]>> => {
        const res = await service_api.get(`/events/${event_id}/scoreboard`);
        return res.data;
    },
    getTrend: async (event_id: string): Promise<UniResponse<TrendItem[]>> => {
        const res = await service_api.get(`/events/${event_id}/trend`);
        return res.data;
    },
    getAnnouncements: async (
        event_id: string,
    ): Promise<UniResponse<EventAnnouncements[]>> => {
        const res = await service_api.get(`/events/${event_id}/announcements`);
        return res.data;
    },
    getOwnWp: async (event_id: string): Promise<UniResponse<string | null>> => {
        const res = await service_api.get(`/events/${event_id}/own_wp`);
        return res.data;
    },
};
