import { create } from "zustand";
import { persist } from "zustand/middleware";
type AuthState = {
	token: string | null;
	adminToken: string | null;
	username: string | null;
	nickname: string | null;
	setToken: (token: string) => void;
	removeToken: () => void;
	setAdminToken: (token: string) => void;
	removeAdminToken: () => void;
	setUsername: (username: string) => void;
	setNickname: (nickname: string) => void;
};

export const useAuthStore = create(
	persist<AuthState>(
		(set) => ({
			token: null,
			adminToken: null,
			username: null,
			nickname: null,
			setToken: (token) => set({ token: token }),
			removeToken: () => set({ token: null }),
			setAdminToken: (token) => set({ adminToken: token }),
			removeAdminToken: () => set({ adminToken: null }),
			setUsername: (username: string) => set({ username: username }),
			setNickname: (nickname: string) => set({ nickname: nickname }),
		}),
		{
			name: "auth-storage",
		},
	),
);
