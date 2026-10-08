/**
 * 路由与鉴权守卫 —— 平台级要求：受保护内容不得闪现，401 必须引导重新登录。
 * 守卫只做「有没有对应作用域的 token」判定；真实权限由后端接口决定（403 会显式展示）。
 */

import type { ReactNode } from "react";
import { Navigate, useLocation } from "react-router";

import { useIsAdminAuthenticated, useIsUserAuthenticated } from "~/auth/store";

function currentPath(): string {
	if (typeof window === "undefined") return "/";
	return `${window.location.pathname}${window.location.search}`;
}

export function RequireUser({ children }: { children: ReactNode }) {
	const authenticated = useIsUserAuthenticated();
	const location = useLocation();
	if (!authenticated) {
		const next = `${location.pathname}${location.search}`;
		return <Navigate to={`/login?next=${encodeURIComponent(next || "/")}`} replace />;
	}
	return <>{children}</>;
}

export function RequireAdmin({ children }: { children: ReactNode }) {
	const authenticated = useIsAdminAuthenticated();
	const location = useLocation();
	if (!authenticated) {
		const next = `${location.pathname}${location.search}`;
		return <Navigate to={`/admin/login?next=${encodeURIComponent(next || "/admin")}`} replace />;
	}
	return <>{children}</>;
}

/** 已登录用户访问登录 / 注册页时回到 `next` 或首页。 */
export function RedirectIfAuthenticated({
	children,
	scope = "user",
	to = "/",
}: {
	children: ReactNode;
	scope?: "user" | "admin";
	to?: string;
}) {
	const user = useIsUserAuthenticated();
	const admin = useIsAdminAuthenticated();
	const authenticated = scope === "admin" ? admin : user;
	if (authenticated) {
		const params = new URLSearchParams(typeof window === "undefined" ? "" : window.location.search);
		const next = params.get("next");
		return <Navigate to={next && next.startsWith("/") ? next : to} replace />;
	}
	return <>{children}</>;
}

export { currentPath };
