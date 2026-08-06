import { type UniResponse, admin_api } from "@/api/axios";

export const adminLoginFn = async ({
    username,
    password,
}: {
    username: string;
    password: string;
}): Promise<UniResponse<string>> => {
    const response = await admin_api.post("/session", { username, password });
    return response.data;
};
