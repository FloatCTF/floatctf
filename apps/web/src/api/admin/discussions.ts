import type { DiscussionComments, Discussions } from "@/entity";
import { type QueryParams, type UniResponse, admin_api } from "@/api/axios";

export const discussionAdminApi = {
    fetch: async (
        params: QueryParams = {},
    ): Promise<UniResponse<Discussions[]>> => {
        const res = await admin_api.get("/discussions", { params });
        return res.data;
    },
    get: async (id: string): Promise<UniResponse<Discussions>> => {
        const res = await admin_api.get(`/discussions/${id}`);
        return res.data;
    },
    remove: async (id_list: string[]): Promise<UniResponse<number>> => {
        const res = await admin_api.delete("/discussions", {
            data: { id_list },
        });
        return res.data;
    },
    getComments: async (
        id: string,
        params: QueryParams = {},
    ): Promise<UniResponse<DiscussionComments[]>> => {
        const res = await admin_api.get(`/discussions/${id}/comments`, {
            params,
        });
        return res.data;
    },
    removeComment: async (
        discussion_id: string,
        comment_id: string,
    ): Promise<UniResponse<null>> => {
        const res = await admin_api.delete(
            `/discussions/${discussion_id}/comments/${comment_id}`,
        );
        return res.data;
    },
};
