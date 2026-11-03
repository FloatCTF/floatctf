import { create } from "zustand";
import { persist } from "zustand/middleware";

import type { MeDto } from "@floatctf/sdk";

/**
 * 会话状态。
 *
 * v2 身份模型下**只有一个会话令牌**：`/api/session` 是唯一登录入口，
 * 管理员与选手是同一个账号体系的两种能力，不再有第二套凭据。
 * 因此 getAdminToken 与 getUserToken 返回同一个值（见 `~/api/client.ts`）。
 *
 * 持久化版本从 1 起：令牌结构变更（v1 → v2 的 JWT 多了 purpose/token_version）
 * 时必须提升版本并清空旧值，否则会出现「前端以为已登录、后端一律 401」。
 */
const SESSION_STORAGE_VERSION = 1;

interface SessionState {
	token: string | null;
	/** `/me` 的响应；含 permissions / grants_all（仅用于界面显隐）。 */
	me: MeDto | null;
	/** 是否已经完成过一次 `/me` 校验（区分「无权限」与「还没拉到」）。 */
	meLoaded: boolean;
	setToken: (token: string | null) => void;
	setMe: (me: MeDto | null) => void;
	/** 登出：清 token **并**清权限，避免下一个账号继承上一个账号的导航可见性。 */
	clear: () => void;
}

export const useSession = create(
	persist<SessionState, [], [], Partial<SessionState>>(
		(set) => ({
			token: null,
			me: null,
			meLoaded: false,
			setToken: (token) =>
				set(
					token === null
						? { token: null, me: null, meLoaded: false }
						: { token },
				),
			setMe: (me) => set({ me, meLoaded: true }),
			clear: () => set({ token: null, me: null, meLoaded: false }),
		}),
		{
			name: "animal-island-session",
			version: SESSION_STORAGE_VERSION,
			merge: (persisted, current) => {
				const data = (persisted ?? {}) as Partial<SessionState>;
				return {
					...current,
					token: data.token ?? null,
					me: data.me ?? null,
					meLoaded: data.meLoaded ?? false,
				};
			},
			migrate: (persisted, version) => {
				if (version < SESSION_STORAGE_VERSION) {
					return { token: null, me: null, meLoaded: false };
				}
				return (persisted ?? {}) as Partial<SessionState>;
			},
		},
	),
);

/** 供非 React 代码（SDK 的 token getter）同步读取。 */
export function currentToken(): string | null {
	return useSession.getState().token;
}

/** 当前账号的有效权限集合（仅用于界面显隐）。 */
export function currentPermissions(): ReadonlySet<string> {
	return new Set(useSession.getState().me?.permissions ?? []);
}

/**
 * 是否具备「平台管理能力」。
 *
 * 判据来自后端 `/me`：持有至少一条权限，或持有全权角色。
 * 前端**不**自造名单 —— 这是导航显隐的判据，不是授权判据。
 */
export function hasPlatformAccess(me: MeDto | null): boolean {
	if (!me) return false;
	return Boolean(me.grants_all) || (me.permissions?.length ?? 0) > 0;
}
