/**
 * 会话状态 —— **归本前端所有**（SDK 从不触碰 `localStorage`、从不导航）。
 *
 * - 两个**互相独立**的 token 作用域：选手 `user` / 管理员 `admin`，各自一个 localStorage 键；
 * - 读写都有异常兜底（隐私模式退化为仅内存会话，不阻断使用）；
 * - 通过 `useSyncExternalStore`（zustand）暴露给 React；
 * - token **绝不**进入 URL / query string，界面与日志也绝不回显 token。
 *
 * 注意：`Users` 实体带 `password` 字段（类型如此），**任何界面都不得渲染它**。
 */

import type { Users } from "@floatctf/sdk/entity";
import { create } from "zustand";

const USER_KEY = "shadcn.user.token";
const ADMIN_KEY = "shadcn.admin.token";

export type AuthScope = "user" | "admin";

function readStored(key: string): string | null {
	try {
		const value = window.localStorage.getItem(key);
		return value && value.length > 0 ? value : null;
	} catch {
		return null;
	}
}

function writeStored(key: string, value: string | null): void {
	try {
		if (value === null) window.localStorage.removeItem(key);
		else window.localStorage.setItem(key, value);
	} catch {
		/* 隐私模式 / 配额不足：退化为内存会话 */
	}
}

export interface AuthState {
	userToken: string | null;
	adminToken: string | null;
	/** 当前选手资料（`GET /users/me`）；未登录为 null。 */
	me: Users | null;
	/** 是否已完成一次会话校验（用于避免受保护内容闪现）。 */
	sessionChecked: boolean;
	setUserToken: (token: string | null) => void;
	setAdminToken: (token: string | null) => void;
	setMe: (me: Users | null) => void;
	setSessionChecked: (checked: boolean) => void;
	/** 只清某一个作用域（401 时用）—— 另一个作用域不受影响。 */
	clearScope: (scope: AuthScope) => void;
	/** 清空全部会话（登出）。 */
	clearAll: () => void;
}

export const useAuthStore = create<AuthState>((set) => ({
	userToken: typeof window === "undefined" ? null : readStored(USER_KEY),
	adminToken: typeof window === "undefined" ? null : readStored(ADMIN_KEY),
	me: null,
	sessionChecked: false,

	setUserToken: (token) => {
		writeStored(USER_KEY, token);
		set({ userToken: token, ...(token === null ? { me: null, sessionChecked: true } : {}) });
	},
	setAdminToken: (token) => {
		writeStored(ADMIN_KEY, token);
		set({ adminToken: token });
	},
	setMe: (me) => set({ me }),
	setSessionChecked: (sessionChecked) => set({ sessionChecked }),
	clearScope: (scope) => {
		if (scope === "admin") {
			writeStored(ADMIN_KEY, null);
			set({ adminToken: null });
			return;
		}
		writeStored(USER_KEY, null);
		set({ userToken: null, me: null, sessionChecked: true });
	},
	clearAll: () => {
		writeStored(USER_KEY, null);
		writeStored(ADMIN_KEY, null);
		set({ userToken: null, adminToken: null, me: null, sessionChecked: true });
	},
}));

/** 非 React 取值（SDK 的 `getUserToken` / `getAdminToken`）。 */
export function getUserToken(): string | null {
	return useAuthStore.getState().userToken;
}

export function getAdminToken(): string | null {
	return useAuthStore.getState().adminToken;
}

export function setUserToken(token: string | null): void {
	useAuthStore.getState().setUserToken(token);
}

export function setAdminToken(token: string | null): void {
	useAuthStore.getState().setAdminToken(token);
}

/** `@floatctf/react` 的 token source（在 render 中调用，必须走 hook 才能响应变化）。 */
export function useUserTokenSource(): string | null {
	return useAuthStore((state) => state.userToken);
}

export function useAdminTokenSource(): string | null {
	return useAuthStore((state) => state.adminToken);
}

export function useMe(): Users | null {
	return useAuthStore((state) => state.me);
}

export function useIsUserAuthenticated(): boolean {
	return useAuthStore((state) => state.userToken !== null);
}

export function useIsAdminAuthenticated(): boolean {
	return useAuthStore((state) => state.adminToken !== null);
}
