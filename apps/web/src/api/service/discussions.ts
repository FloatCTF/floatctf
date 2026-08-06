import type { DiscussionComments, Discussions } from "@/entity";
import { type QueryParams, type UniResponse, service_api } from "../axios";

export const discussionServiceApi = {
    fetch: async (
        params: QueryParams = {},
    ): Promise<UniResponse<Discussions[]>> => {
        const res = await service_api.get("/discussions", { params });
        return res.data;
    },
    get: async (id: string): Promise<UniResponse<Discussions>> => {
        const res = await service_api.get(`/discussions/${id}`);
        return res.data;
    },
    create: async (data: {
        title: string;
        content: string;
    }): Promise<UniResponse<Discussions>> => {
        const res = await service_api.post("/discussions", data);
        return res.data;
    },
    patch: async (
        data: Partial<Discussions>,
    ): Promise<UniResponse<Discussions>> => {
        const res = await service_api.patch(`/discussions/${data.id}`, data);
        return res.data;
    },
    remove: async (id: string): Promise<UniResponse<null>> => {
        const res = await service_api.delete(`/discussions/${id}`);
        return res.data;
    },
    like: async (id: string): Promise<UniResponse<null>> => {
        const res = await service_api.post(`/discussions/${id}/like`);
        return res.data;
    },
    unlike: async (id: string): Promise<UniResponse<null>> => {
        const res = await service_api.delete(`/discussions/${id}/like`);
        return res.data;
    },
    getComments: async (
        id: string,
        params: QueryParams = {},
    ): Promise<UniResponse<DiscussionComments[]>> => {
        const res = await service_api.get(`/discussions/${id}/comments`, {
            params,
        });
        return res.data;
    },
    createComment: async (
        discussion_id: string,
        data: { content: string; parent_id?: string },
    ): Promise<UniResponse<DiscussionComments>> => {
        const res = await service_api.post(
            `/discussions/${discussion_id}/comments`,
            data,
        );
        return res.data;
    },
    patchComment: async (
        discussion_id: string,
        comment_id: string,
        data: { content: string },
    ): Promise<UniResponse<DiscussionComments>> => {
        const res = await service_api.patch(
            `/discussions/${discussion_id}/comments/${comment_id}`,
            data,
        );
        return res.data;
    },
    deleteComment: async (
        discussion_id: string,
        comment_id: string,
    ): Promise<UniResponse<null>> => {
        const res = await service_api.delete(
            `/discussions/${discussion_id}/comments/${comment_id}`,
        );
        return res.data;
    },
};
