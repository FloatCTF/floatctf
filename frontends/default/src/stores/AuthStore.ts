import { create } from "zustand";
import { persist } from "zustand/middleware";

/**
 * 持久化结构的版本号。
 *
 * v1 → v2 的**唯一**原因：后端 JWT 结构从 `{ sub, role, exp }` 变成
 * `{ sub, purpose, token_version, exp }`，v1 令牌会被后端**拒绝**。
 * 若不清掉浏览器里已持久化的 v1 令牌，用户会处于「前端以为已登录、后端一律 401」
 * 的状态，表现为白屏或登录页反复跳转 —— 比直接要求重新登录糟糕得多。
 */
export const AUTH_STORAGE_VERSION = 2;

type AuthState = {
	/** 会话令牌（v2 起只有一个；administrator 与 player 共用同一账号体系）。 */
	token: string | null;
	/**
	 * 管理端令牌。
	 *
	 * v2 里它与 `token` 是**同一个后端会话**，但默认前端仍把两个 axios 实例的
	 * 凭据分开保存（`serviceHttp` / `adminHttp` 各自的 header 注入）。
	 * 保留两个字段是为了不破坏既有请求管线；它们由同一次登录一起写入。
	 */
	adminToken: string | null;
	username: string | null;
	nickname: string | null;
	/**
	 * 当前账号的有效权限（来自 `GET /api/users/me`）。
	 *
	 * **只用于界面显隐**，不是授权依据：真正的判定在服务端每个请求上
	 * （`RequirePermission<P>`）。因此这里即使为空也只影响导航，
	 * 不会让用户越权或被误拒。
	 */
	permissions: string[];
	/** 是否持有全权角色（内置 super-admin）。 */
	grantsAll: boolean;
	/** 权限是否已经从 `/me` 拉取过（区分「空权限」与「还没拉」）。 */
	permissionsLoaded: boolean;
	setToken: (token: string) => void;
	removeToken: () => void;
	setAdminToken: (token: string) => void;
	removeAdminToken: () => void;
	setUsername: (username: string) => void;
	setNickname: (nickname: string) => void;
	/** 由 `/me` 的响应填写。 */
	setPermissions: (permissions: string[], grantsAll: boolean) => void;
	/** 登出时清空权限，避免下一个账号继承上一个账号的导航可见性。 */
	removePermissions: () => void;
};

export const useAuthStore = create(
	persist<AuthState, [], [], Partial<AuthState>>(
		(set) => ({
			token: null,
			adminToken: null,
			username: null,
			nickname: null,
			permissions: [],
			grantsAll: false,
			permissionsLoaded: false,
			setToken: (token) => set({ token: token }),
			// 登出必须**同时**清权限：否则下一个登录的账号会短暂看到上一个账号的
			// 管理入口（虽然点进去也会被服务端 403，但那是不该出现的界面状态）。
			removeToken: () =>
				set({
					token: null,
					permissions: [],
					grantsAll: false,
					permissionsLoaded: false,
				}),
			setAdminToken: (token) => set({ adminToken: token }),
			removeAdminToken: () =>
				set({
					adminToken: null,
					permissions: [],
					grantsAll: false,
					permissionsLoaded: false,
				}),
			setUsername: (username: string) => set({ username: username }),
			setNickname: (nickname: string) => set({ nickname: nickname }),
			setPermissions: (permissions, grantsAll) =>
				set({ permissions, grantsAll, permissionsLoaded: true }),
			removePermissions: () =>
				set({
					permissions: [],
					grantsAll: false,
					permissionsLoaded: false,
				}),
		}),
		{
			name: "auth-storage",
			version: AUTH_STORAGE_VERSION,
			merge: (persisted, current) => {
				// 只接受**数据字段**，函数一律用 store 自己的实现。
				// 否则升级后旧持久化对象里的旧函数会覆盖新实现。
				const data = (persisted ?? {}) as Partial<AuthState>;
				return {
					...current,
					token: data.token ?? null,
					adminToken: data.adminToken ?? null,
					username: data.username ?? null,
					nickname: data.nickname ?? null,
					permissions: data.permissions ?? [],
					grantsAll: data.grantsAll ?? false,
					permissionsLoaded: data.permissionsLoaded ?? false,
				};
			},
			migrate: (persisted, version) => {
				const data = (persisted ?? {}) as Partial<AuthState>;
				if (version < AUTH_STORAGE_VERSION) {
					// ★ 清空令牌：v1 令牌会被后端拒绝，保留它只会制造
					// 「以为已登录却处处 401」的状态。要求重新登录一次是
					// 一次性且预期内的代价（已写入计划文档的发布说明）。
					return {
						token: null,
						adminToken: null,
						username: null,
						nickname: null,
						permissions: [],
						grantsAll: false,
						permissionsLoaded: false,
					} satisfies Partial<AuthState>;
				}
				return data;
			},
		},
	),
);

/** 供非 React 代码（路由守卫、axios 注入）读取权限集合。 */
export function permissionsFromStore(): ReadonlySet<string> {
	return new Set(useAuthStore.getState().permissions);
}
