import {
	Outlet,
	createFileRoute,
	redirect,
	useLocation,
} from "@tanstack/react-router";

import { HierarchicalSideBar, ServiceHeader } from "@/components";
import { serviceNavigation } from "@/navigation";
import { useAuthStore } from "@/stores/AuthStore";

export const Route = createFileRoute("/service")({
	// 统一兜底守卫：/service 下所有页面都要求用户 token（未登录 → 登录页 /）。
	// 此前只有部分子路由自带 ServiceRouteGuard，漏声明的页面会渲染出空壳 + 一串 401，
	// 容易被误判成「功能坏了」。
	beforeLoad: async () => {
		const authStore = useAuthStore.getState();
		if (!authStore.token) {
			throw redirect({ to: "/" });
		}
	},
	component: RouteComponent,
});

function RouteComponent() {
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
	const authStore = useAuthStore.getState();
	if (!authStore.token) {
		return redirect({ to: "/" });
	}
};

export const ServiceRouteGuardWithRedirect = async () => {
	const authStore = useAuthStore.getState();
	if (authStore.token) {
		return redirect({ to: "/service/top" });
	}
};
