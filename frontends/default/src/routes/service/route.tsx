import {
	Outlet,
	createFileRoute,
	redirect,
	useLocation,
	useNavigate,
} from "@tanstack/react-router";

import { useEffect, useRef } from "react";

import { serviceApi } from "@/api";
import { HierarchicalSideBar, ServiceHeader } from "@/components";
import { serviceNavigation } from "@/navigation";
import { isSignedIn } from "@/session";
import { useAuthStore } from "@/stores/AuthStore";
import { useSessionStore } from "@/stores/SessionStore";

export const Route = createFileRoute("/service")({
	// 统一兜底守卫：/service 下所有页面都要求已登录（未登录 → 登录页 /）。
	// 统一会话部署以服务端 Cookie 会话为准；旧版部署以平台 JWT 为准（见 `@/session`）。
	beforeLoad: async () => {
		if (!(await isSignedIn())) {
			throw redirect({ to: "/" });
		}
	},
	component: RouteComponent,
});

/**
 * 旧版 JWT 模式：把已持有的平台 JWT 衔接为服务端 SSO 会话（OIDC 第一方登录态）。
 *
 * 统一 Cookie 会话部署上主站会话本身就是身份会话，不需要衔接。接口幂等，OIDC 未启用时返回 404，均静默忽略。
 */
function useSsoBridge() {
	const token = useAuthStore((s) => s.token);
	const mode = useSessionStore((s) => s.mode);
	useEffect(() => {
		if (token && mode === "legacy") {
			void serviceApi.oidc.establishSso().catch(() => undefined);
		}
	}, [token, mode]);
}

/**
 * 统一会话：另一标签页退出 / 切换账号（BroadcastChannel 或 401 重新同步）后，
 * 本标签页不能继续停留在上一个账号的页面上——退出则回登录页，换人则回首页并重新加载数据。
 */
function useSessionWatcher() {
	const navigate = useNavigate();
	const mode = useSessionStore((s) => s.mode);
	const userId = useSessionStore((s) => s.status?.user?.id ?? null);
	const previous = useRef(userId);
	useEffect(() => {
		if (mode !== "cookie") return;
		if (userId === null) {
			void navigate({ to: "/" });
		} else if (previous.current && previous.current !== userId) {
			void navigate({ to: "/service/top" });
		}
		previous.current = userId;
	}, [mode, userId, navigate]);
}

function RouteComponent() {
	useSsoBridge();
	useSessionWatcher();
	const location = useLocation();

	const hideSidebarPatterns = [/^\/service\/events\/.+/];
	const shouldHideSidebar = hideSidebarPatterns.some((pattern) =>
		pattern.test(location.pathname),
	);
	return (
		<div className="h-full w-full flex flex-col">
			<ServiceHeader />
			<div className="flex flex-row flex-1 min-h-0">
				{!shouldHideSidebar && ( // 👈 在这里判断
					<div className="border-right h-full pl-2 w-fit flex-shrink-0 min-h-0 overflow-y-auto">
						<HierarchicalSideBar
							sections={serviceNavigation}
							ariaLabel="Service navigation"
						/>
					</div>
				)}
				<div className="flex-1 h-full p-2 min-h-0 overflow-auto">
					<Outlet />
				</div>
			</div>
		</div>
	);
}
export const ServiceRouteGuard = async () => {
	if (!(await isSignedIn())) {
		return redirect({ to: "/" });
	}
};

export const ServiceRouteGuardWithRedirect = async () => {
	if (await isSignedIn()) {
		return redirect({ to: "/service/top" });
	}
};
