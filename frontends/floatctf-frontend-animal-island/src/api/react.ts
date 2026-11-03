/**
 * `@floatctf/react` 绑定。
 *
 * 只在**这里**调用一次 `createFloatCTFReact`，把 SDK 客户端与本前端的令牌来源接上。
 * 页面通过 `useAwdEventStream` 等 hook 使用它 —— 这样 SSE 的失效列表
 * （`AWD_PLAYER_QUERY_KEYS` / `AWD_ADMIN_QUERY_KEYS`）与页面实际读取的 query key
 * 来自同一份常量，不会漂移。
 *
 * 为什么这件事重要：SSE 连上后**轮询回退会停止**（两者互斥）。如果事件流的失效列表比
 * 页面真正读的 key 窄，面板就会在整场比赛里只靠手动刷新更新 —— 而连接看起来是正常的。
 */
import { createFloatCTFReact } from "@floatctf/react";

import { useSession } from "~/auth/store";
import type { FloatCTFClient } from "@floatctf/sdk";

/** 选手与管理员共用同一个会话令牌（v2 起平台只有一个会话 JWT）。 */
function useSessionToken(): string | null {
	return useSession((s) => s.token);
}

/**
 * 为指定客户端创建绑定。
 *
 * 因为 SDK 客户端是在 `mount(context)` 里按 `apiBaseUrl` 创建的，
 * 绑定也必须跟着客户端走，不能做成模块级单例。
 */
export function createBindings(client: FloatCTFClient) {
	return createFloatCTFReact({
		client,
		useUserToken: useSessionToken,
		// 管理员也用同一个会话令牌（v2 的单一会话模型）。
		useAdminToken: useSessionToken,
	});
}

export type Bindings = ReturnType<typeof createBindings>;
export {
	AWD_ADMIN_QUERY_KEYS,
	AWD_PLAYER_QUERY_KEYS,
	invalidateAwdQueries,
} from "@floatctf/react";
