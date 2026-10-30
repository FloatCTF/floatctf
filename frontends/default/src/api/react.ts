/**
 * Default Frontend 的 React 绑定接线（pages 通过 `@/api/react` 使用）。
 *
 * `@floatctf/react` 是**可选**的 headless 包：它不知道 token 存在哪里，也不知道
 * 用哪个状态库。这里把本前端的 Zustand 选择器注入进去，得到与迁移前
 * `@/hooks/*`、`@/api/queries` **同名同签名**的 hook 与 query 工厂，
 * 因此页面代码零改动。
 *
 * 非 React 前端完全不使用本文件——它们直接用 `@floatctf/sdk`。
 */

import { createFloatCTFReact } from "@floatctf/react";

import { useAuthStore } from "@/stores/AuthStore";
import { useSessionStore } from "@/stores/SessionStore";
import { client } from "./client";

/**
 * 选手端实时连接的「令牌」：旧版模式是平台 JWT；Cookie 会话模式下是「当前账号 + CSRF」组成的
 * 不透明标记——只用于触发连接重建，SDK 在会话头存在时不会把它当作 Bearer 发送。
 */
function useUserConnectionKey(): string | null {
	const legacy = useAuthStore((s) => s.token);
	const mode = useSessionStore((s) => s.mode);
	const status = useSessionStore((s) => s.status);
	if (mode === "cookie") {
		return status?.authenticated && status.user && status.csrf_token
			? `session:${status.user.id}:${status.csrf_token.slice(0, 8)}`
			: null;
	}
	return legacy;
}

const bindings = createFloatCTFReact({
	client,
	useUserToken: useUserConnectionKey,
	useAdminToken: () => useAuthStore((s) => s.adminToken),
});

export const {
	// 实时事件流
	useAwdEventStream,
	useAdminAwdEventStream,
	useAwdpEventStream,
	useAwdpRunStream,
	// query options 工厂
	eventInfoQueryOptions,
	challengeQueryOptions,
	challengeInstanceQueryOptions,
	systemInformationQueryOptions,
} = bindings;
