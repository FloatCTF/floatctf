import { ADMIN_API_URL, API_URL } from "@/config";
import { router } from "@/main";
import { useAuthStore } from "@/stores/AuthStore";

import axios from "axios";
export const service_api = axios.create({
	baseURL: API_URL,
});

export const admin_api = axios.create({
	baseURL: ADMIN_API_URL,
});

admin_api.interceptors.request.use((config) => {
	const token = useAuthStore.getState().adminToken;
	if (token) {
		config.headers.Authorization = `Bearer ${token}`;
	}
	return config;
});
admin_api.interceptors.response.use(
	(response) => {
		// console.log(response);
		return response;
	},

	(error) => {
		if (error.response.status === 401) {
			useAuthStore.getState().removeAdminToken();
			router.navigate({ to: "/admin" });
		}
		console.log(error);
		return Promise.reject(error);
	},
);
service_api.interceptors.request.use((config) => {
	const token = useAuthStore.getState().token;
	if (token) {
		config.headers.Authorization = `Bearer ${token}`;
	}
	return config;
});
service_api.interceptors.response.use(
	(response) => {
		// console.log(response);
		return response;
	},
	(error) => {
		if (error.response.status === 401) {
			useAuthStore.getState().removeToken();
			router.navigate({ to: "/" });
		}
		console.log(error);
		return Promise.reject(error);
	},
);

export type QueryParams = {
	offset?: number;
	limit?: number;
	page?: number;
	total?: number;
	filter?: string;
};

export type UniResponse<T> = {
	code: number;
	message: string;
	data?: T;
	meta?: QueryParams;
};
