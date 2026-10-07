import {
	Outlet,
	createFileRoute,
	redirect,
	useLocation,
} from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { AdminHeader, HierarchicalSideBar } from "@/components";
import { adminIgnoreRoutes, adminNavigation } from "@/navigation";
import { useAuthStore } from "@/stores/AuthStore";

export const Route = createFileRoute("/admin")({
	// 统一兜底守卫：/admin 下所有子路由都要求 adminToken（此前只靠各子路由自己声明，
	// /admin/database、/admin/version、/admin/terminal、/admin/docker 等页面没有守卫）。
	// 注意登录页 /admin 本身在 adminIgnoreRoutes 里 → 必须跳过，否则
	// redirect({ to: "/admin" }) 会自我循环（这正是该守卫此前被注释掉的原因）。
	beforeLoad: async ({ location }) => {
		if (adminIgnoreRoutes.includes(location.pathname)) {
			return;
		}
		const authStore = useAuthStore.getState();
		if (!authStore.adminToken) {
			throw redirect({ to: "/admin" });
		}
	},
	component: RouteComponent,
});

function RouteComponent() {
	useTitle("Admin | FloatCTF");
	const location = useLocation();
	if (adminIgnoreRoutes.includes(location.pathname)) {
		return <Outlet />;
	}

	return (
		<div className="flex flex-col h-full">
			<AdminHeader />

			<div className="flex flex-row h-full">
				<div className="border-right h-full pl-2 w-fit overflow-y-auto">
					<HierarchicalSideBar sections={adminNavigation} />
				</div>
				<div className="p-2 w-full flex-1 min-w-0 overflow-auto">
					<Outlet />
				</div>
			</div>
		</div>
	);
}
export const AdminRouteGuard = async () => {
	const authStore = useAuthStore.getState();
	if (!authStore.adminToken) {
		return redirect({ to: "/admin" });
	}
};

export const AdminRouteGuardWithRedirect = async () => {
	const authStore = useAuthStore.getState();
	if (authStore.adminToken) {
		return redirect({ to: "/admin/dashboard" });
	}
};
