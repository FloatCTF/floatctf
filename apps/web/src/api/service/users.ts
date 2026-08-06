import type { Users } from "@/entity";
import { type UniResponse, service_api } from "../axios";

export const userServiceApi = {
    getMe: async (): Promise<UniResponse<Users>> => {
        const response = await service_api.get("/users/me");
        return response.data;
    },
    patchMe: async (data: Partial<Users>): Promise<UniResponse<Users>> => {
        const response = await service_api.patch("/users/me", data);
        return response.data;
    },
    login: async ({
        username,
        password,
    }: {
        username: string;
        password: string;
    }): Promise<UniResponse<string>> => {
        const response = await service_api.post("/users/session", {
            username,
            password,
        });
        return response.data;
    },
    register: async ({
        username,
        password,
        nickname,
        email,
    }: {
        username: string;
        password: string;
        nickname: string;
        email: string;
    }): Promise<UniResponse<string>> => {
        const response = await service_api.post("/users", {
            username,
            password,
            nickname,
            email,
        });
        return response.data;
    },
    resetPassword: async ({
        username,
        email,
    }: {
        username?: string;
        email?: string;
    }): Promise<UniResponse<string>> => {
        const response = await service_api.post("/users/reset_password", {
            username,
            email,
        });
        return response.data;
    },
    reset: async ({
        token,
        password,
        confirmed_password,
    }: {
        token: string;
        password: string;
        confirmed_password: string;
    }): Promise<UniResponse<string>> => {
        const response = await service_api.post(`/users/reset?token=${token}`, {
            password,
            confirmed_password,
        });
        return response.data;
    },
};
