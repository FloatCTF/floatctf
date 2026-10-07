/**
 * **Default Frontend 的 SDK 接线层**（前端拥有，SDK 不拥有）。
 *
 * 这里做三件事——**全部是 UI 侧决策**，SDK 一概不参与：
 * 1. token 从哪来：Zustand `AuthStore`（含既有 localStorage 兼容）
 * 2. 401 之后干什么：清对应 token + 跳转到既有登录入口（选手 `/`、管理 `/admin`）
 * 3. 暴露绑定好的客户端给页面使用
 *
 * 迁移说明：这一段逻辑原先散在 `apps/web/src/api/axios.ts` 里（直接 import router
 * 与 AuthStore）。现在传输机制在 `@floatctf/sdk`，**UI 反应留在这里**，
 * 因此外部前端可以完全换成自己的写法（Context、内存、别的状态库）。
 */

import { createFloatCTFClient } from "@floatctf/sdk";

import { router } from "@/router";
import { useAuthStore } from "@/stores/AuthStore";
import { API_URL } from "@/config";

export const client = createFloatCTFClient({
	baseUrl: API_URL,
	getUserToken: () => useAuthStore.getState().token,
	getAdminToken: () => useAuthStore.getState().adminToken,
	onUnauthorized: ({ scope, error }) => {
		// 与迁移前逐字一致的行为：清 token → 回到该端的登录入口。
		if (scope === "admin") {
			useAuthStore.getState().removeAdminToken();
			void router.navigate({ to: "/admin" });
		} else {
			useAuthStore.getState().removeToken();
			void router.navigate({ to: "/" });
		}
		console.log(error);
	},
	onError: (error) => {
		// 迁移前 axios 拦截器会 console.log(error)；保持同样的可见性，不改变行为。
		console.log(error);
	},
});
