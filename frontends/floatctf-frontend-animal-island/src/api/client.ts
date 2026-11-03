import type { FloatCTFMountContext } from "@floatctf/frontend-runtime";
import {
	createFloatCTFClient,
	type FloatCTFClient,
} from "@floatctf/sdk";

import { currentToken, useSession } from "~/auth/store";

/** 作用域：v2 下两者共用同一个会话令牌，但错误提示要能区分。 */
export type Scope = "user" | "admin";

export interface Runtime {
	client: FloatCTFClient;
	apiBaseUrl: string;
	frontendVersion: string;
	platformVersion: string;
	capabilities: ReadonlySet<string>;
}

/** 会话失效时由 UI 层注册的处理器（跳登录页 + 说明原因）。 */
let onSessionExpired: ((scope: Scope, reason: string) => void) | null = null;
/** 403 时由 UI 层注册的处理器（**只提示，绝不清会话**）。 */
let onForbidden: ((message: string) => void) | null = null;

export function registerSessionHandlers(handlers: {
	onSessionExpired?: (scope: Scope, reason: string) => void;
	onForbidden?: (message: string) => void;
}): void {
	onSessionExpired = handlers.onSessionExpired ?? null;
	onForbidden = handlers.onForbidden ?? null;
}

export function createRuntime(context: FloatCTFMountContext): Runtime {
	const client = createFloatCTFClient({
		baseUrl: context.apiBaseUrl,
		// v2 单一会话：两个作用域读同一个令牌。
		getUserToken: currentToken,
		getAdminToken: currentToken,
		onUnauthorized: ({ scope }) => {
			// 401 = 凭据问题（无效 / 被吊销 / 用途不符）→ 清会话并引导重新登录。
			useSession.getState().clear();
			onSessionExpired?.(scope === "admin" ? "admin" : "user", "登录状态已失效");
		},
		onForbidden: ({ error }) => {
			// ★ 403 = 已认证但**不允许**（账号禁用 / 缺权限 / 对象级拒绝）。
			// 绝不在这里清 token：重新登录解决不了权限问题，
			// 清掉只会把用户莫名其妙踢出去。
			onForbidden?.(error.platformMessage || "没有权限执行该操作");
		},
	});

	return {
		client,
		apiBaseUrl: context.apiBaseUrl,
		frontendVersion: context.frontendVersion,
		platformVersion: context.platformVersion,
		capabilities: new Set(context.capabilities),
	};
}
