/**
 * 运行时装配 —— 一次 `mount(context)` 一套完全独立的客户端 / QueryClient / React 绑定。
 *
 * 边界（AI-FRONTEND-GUIDE §4）：本文件只 import 公共包（`@floatctf/sdk` /
 * `@floatctf/frontend-runtime` / `@floatctf/react`）与自身源码；不 import
 * `frontends/default`、`apps/web`、`packages` 内部源码，也不用 `@/` 私有 alias。
 */

import type { FloatCTFMountContext } from "@floatctf/frontend-runtime";
import { createFloatCTFReact } from "@floatctf/react";
import { createFloatCTFClient, type FloatCTFClient } from "@floatctf/sdk";
import { QueryClient, useQueryClient } from "@tanstack/react-query";
import { createContext, useContext } from "react";

import {
	getAdminToken,
	getUserToken,
	setAdminToken,
	setUserToken,
	useAdminTokenSource,
	useUserTokenSource,
} from "~/auth/store";

import { installEnvelopeGuard } from "./call";

export type ReactBindings = ReturnType<typeof createFloatCTFReact>;

export interface AppRuntime {
	context: FloatCTFMountContext;
	client: FloatCTFClient;
	bindings: ReactBindings;
	queryClient: QueryClient;
}

export const RuntimeContext = createContext<AppRuntime | null>(null);

/**
 * 401 之后做什么由本前端决定（SDK 不导航）。装配时由路由层注册处理器，
 * 避免 API 层直接依赖路由；`next` 让用户重新登录后回到原处。
 */
let onScopeExpired: ((scope: "user" | "admin", next: string) => void) | null = null;

export function setUnauthorizedHandler(
	handler: ((scope: "user" | "admin", next: string) => void) | null,
): void {
	onScopeExpired = handler;
}

function currentLocation(): string {
	if (typeof window === "undefined") return "/";
	return `${window.location.pathname}${window.location.search}`;
}

function createQueryClient(): QueryClient {
	return new QueryClient({
		defaultOptions: {
			queries: {
				staleTime: 15_000,
				gcTime: 5 * 60_000,
				retry: (failureCount, error) => {
					const status = (error as { httpStatus?: number }).httpStatus;
					if (status === 401 || status === 403 || status === 404) return false;
					return failureCount < 2;
				},
				refetchOnWindowFocus: false,
			},
			mutations: { retry: false },
		},
	});
}

export function createRuntime(context: FloatCTFMountContext): AppRuntime {
	const client = createFloatCTFClient({
		baseUrl: context.apiBaseUrl,
		// adminBaseUrl 默认 `${baseUrl}/admin`
		getUserToken,
		getAdminToken,
		onUnauthorized: ({ scope }) => {
			// 清对应作用域的 token（另一个不受影响），再交给路由层引导登录。
			const next = currentLocation();
			if (scope === "admin") setAdminToken(null);
			else setUserToken(null);
			onScopeExpired?.(scope === "admin" ? "admin" : "user", next);
		},
	});

	const bindings = createFloatCTFReact({
		client,
		useUserToken: useUserTokenSource,
		useAdminToken: useAdminTokenSource,
	});

	// HTTP 200 + code !== 0 的平台业务失败 → rejection（否则界面会「静默成功」）。
	installEnvelopeGuard(client);

	return { context, client, bindings, queryClient: createQueryClient() };
}

export function useRuntime(): AppRuntime {
	const runtime = useContext(RuntimeContext);
	if (!runtime) throw new Error("useRuntime 必须在 <RuntimeProvider> 内使用");
	return runtime;
}

export function useClient(): FloatCTFClient {
	return useRuntime().client;
}

export function useBindings(): ReactBindings {
	return useRuntime().bindings;
}

/** 与 `useQueryClient()` 等价，但语义上明确「这是本前端的 QueryClient」。 */
export function useAppQueryClient(): QueryClient {
	return useQueryClient();
}

/**
 * 用 `context.assetBaseUrl` 解析本前端自带资产（AI-FRONTEND-GUIDE §7.2 强制）。
 * 生产制品路径带版本号且 immutable，绝不能硬编码站点根。
 */
export function assetUrl(context: Pick<FloatCTFMountContext, "assetBaseUrl">, path: string): string {
	const base = context.assetBaseUrl.replace(/\/+$/, "");
	return `${base}/${path.replace(/^\/+/, "")}`;
}
