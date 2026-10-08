/**
 * 公开页面外壳（登录 / 注册 / 找回密码）—— 居中卡片，无侧边栏。
 * 与选手端 / 管理端工作区相互独立，任何 token 都不会在这里泄漏进 URL。
 */

import type { ReactNode } from "react";
import { Link, Outlet } from "react-router";

import { useIsAdminAuthenticated } from "~/auth/store";
import { Button } from "~/components/ui/button";

export function PublicShell({ children }: { children?: ReactNode }) {
	const isAdmin = useIsAdminAuthenticated();
	return (
		<div className="flex min-h-svh flex-col bg-background">
			<header className="flex h-14 items-center justify-between border-b px-4 lg:px-6">
				<Link to="/" className="flex items-center gap-2">
					<span className="flex size-7 items-center justify-center rounded-md bg-primary text-xs font-semibold text-primary-foreground">
						F
					</span>
					<span className="text-sm font-semibold">FloatCTF</span>
					<span className="hidden text-xs text-muted-foreground sm:inline">
						Shadcn Console
					</span>
				</Link>
				<Button variant="ghost" size="sm" asChild>
					<Link to={isAdmin ? "/admin" : "/admin/login"}>
						{isAdmin ? "返回控制台" : "管理端登录"}
					</Link>
				</Button>
			</header>
			<main className="flex flex-1 items-center justify-center px-4 py-10">
				{children ?? <Outlet />}
			</main>
			<footer className="border-t px-4 py-4 text-center text-xs text-muted-foreground">
				FloatCTF · 可插拔前端 <span className="font-mono">shadcn@0.1.0</span>
			</footer>
		</div>
	);
}

/** 认证页面统一卡片容器。 */
export function AuthCard({
	title,
	description,
	children,
	footer,
	wide = false,
}: {
	title: string;
	description?: ReactNode;
	children: ReactNode;
	footer?: ReactNode;
	wide?: boolean;
}) {
	return (
		<div className={wide ? "w-full max-w-lg" : "w-full max-w-md"}>
			<div className="rounded-xl border bg-card p-6 shadow-sm">
				<div className="mb-5 space-y-1">
					<h1 className="text-lg font-semibold">{title}</h1>
					{description ? <p className="text-sm text-muted-foreground">{description}</p> : null}
				</div>
				{children}
			</div>
			{footer ? <div className="mt-4 text-center text-sm">{footer}</div> : null}
		</div>
	);
}
