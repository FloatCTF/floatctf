import type { Users } from "../../entity/index.js";
import type { FloatCTFHttpClient } from "../../transport.js";
import type { UniResponse } from "../../transport.js";

export function createUserServiceApi(http: FloatCTFHttpClient) {
	return {
	    getMe: async (): Promise<UniResponse<Users>> => {
	        const response = await http.get("/users/me");
	        return response.data;
	    },
	    patchMe: async (data: Partial<Users>): Promise<UniResponse<Users>> => {
	        const response = await http.patch("/users/me", data);
	        return response.data;
	    },
	    login: async ({
	        username,
	        password,
	    }: {
	        username: string;
	        password: string;
	    }): Promise<UniResponse<string>> => {
	        const response = await http.post("/users/session", {
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
	        const response = await http.post("/users", {
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
	        const response = await http.post("/users/reset_password", {
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
	        const response = await http.post(`/users/reset?token=${token}`, {
	            password,
	            confirmed_password,
	        });
	        return response.data;
	    },
};
}

export type UserServiceApi = ReturnType<typeof createUserServiceApi>;
