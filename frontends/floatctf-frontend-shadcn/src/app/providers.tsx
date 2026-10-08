/**
 * 应用级 Provider 装配 + 会话引导 + 渲染错误边界。
 * RootLayout 位于路由树内（命令面板需要 router 上下文）。
 */

import { Component, useEffect, type ErrorInfo, type ReactNode } from "react";
import { Outlet } from "react-router";
import { QueryClientProvider, useQuery } from "@tanstack/react-query";
import { TooltipProvider } from "~/components/ui/tooltip";

import { call } from "~/api/call";
import { RuntimeContext, type AppRuntime, useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { useAuthStore } from "~/auth/store";
import { ConfirmProvider } from "~/components/app/confirm";
import { ErrorBlock } from "~/components/app/states";
import { Toaster } from "~/components/ui/sonner";
import { CommandPalette } from "./command-palette";

export function AppProviders({
	runtime,
	children,
}: {
	runtime: AppRuntime;
	children: ReactNode;
}) {
	return (
		<RuntimeContext.Provider value={runtime}>
			<QueryClientProvider client={runtime.queryClient}>
				<TooltipProvider delayDuration={200}>
					<ConfirmProvider>{children}</ConfirmProvider>
				</TooltipProvider>
			</QueryClientProvider>
		</RuntimeContext.Provider>
	);
}

/**
 * 会话引导：有选手 token 时拉一次 `/users/me`，把资料写进 store（供顶栏 / 资料页使用）。
 * 401 会由 transport 的 `onUnauthorized` 清 token 并跳登录；其它错误只标记「已检查」，
 * 不阻断页面（页面自己的 query 会各自显示错误）。
 */
export function SessionBootstrap() {
	const client = useClient();
	const userToken = useAuthStore((state) => state.userToken);
	const setMe = useAuthStore((state) => state.setMe);
	const setSessionChecked = useAuthStore((state) => state.setSessionChecked);

	const query = useQuery({
		queryKey: qk.auth.me(),
		queryFn: () => call(client.service.users.getMe(), "当前用户信息"),
		enabled: userToken !== null,
		retry: false,
		staleTime: 60_000,
	});

	useEffect(() => {
		if (userToken === null) {
			setMe(null);
			setSessionChecked(true);
			return;
		}
		if (query.isSuccess) {
			setMe(query.data ?? null);
			setSessionChecked(true);
		} else if (query.isError) {
			setSessionChecked(true);
		}
	}, [userToken, query.isSuccess, query.isError, query.data, setMe, setSessionChecked]);

	return null;
}

interface ErrorBoundaryState {
	error: Error | null;
}

/** 渲染期异常兜底：绝不留白屏，给出可见原因与恢复入口。 */
export class ErrorBoundary extends Component<{ children: ReactNode }, ErrorBoundaryState> {
	state: ErrorBoundaryState = { error: null };

	static getDerivedStateFromError(error: Error): ErrorBoundaryState {
		return { error };
	}

	componentDidCatch(error: Error, info: ErrorInfo): void {
		// 只记录到控制台，便于排查；不上报任何外部服务。
		console.error("[floatctf-shadcn] 渲染异常", error, info.componentStack);
	}

	render(): ReactNode {
		if (this.state.error) {
			return (
				<div className="mx-auto max-w-2xl p-6">
					<ErrorBlock
						error={this.state.error}
						title="界面渲染出错"
						onRetry={() => this.setState({ error: null })}
					/>
					<p className="mt-3 text-center text-xs text-muted-foreground">
						如果反复出现，请刷新页面；已登录状态不会丢失。
					</p>
				</div>
			);
		}
		return this.props.children;
	}
}

/** 路由根布局：所有页面 + 全局命令面板 + toast 宿主。 */
export function RootLayout() {
	return (
		<>
			<SessionBootstrap />
			<Outlet />
			<CommandPalette />
			<Toaster />
		</>
	);
}
