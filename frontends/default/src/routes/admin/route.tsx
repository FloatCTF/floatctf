import {
	Outlet,
	createFileRoute,
	redirect,
	useLocation,
} from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { useQuery } from "@tanstack/react-query";
import { useEffect, useMemo } from "react";

import { AdminHeader, HierarchicalSideBar } from "@/components";
import { adminIgnoreRoutes, adminNavigation } from "@/navigation";
import type { NavigationVisibilityContext } from "@/navigation";
import { serviceApi } from "@/api";
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
	const authStore = useAuthStore();

	// 权限感知导航的数据来源：`/me` 回传本次请求解析出的**有效权限**。
	// 与 Header 共用同一个 queryKey，react-query 会去重，不会多打一次请求。
	const { data } = useQuery({
		queryKey: ["profile"],
		queryFn: () => serviceApi.users.getMe(),
		enabled: !adminIgnoreRoutes.includes(location.pathname),
	});
	const me = data?.data;

	useEffect(() => {
		if (me) {
			authStore.setPermissions(me.permissions ?? [], Boolean(me.grants_all));
		}
	}, [me, authStore]);

	const visibility = useMemo<NavigationVisibilityContext>(
		() => ({
			permissions: new Set(authStore.permissions),
			// `features` 是**平台能力**（赛制等），与用户权限是两回事（计划 §7.1）。
			// 这里留空表示「不按平台能力裁剪导航」；需要时由 API 填充。
			features: new Set<string>(),
		}),
		[authStore.permissions],
	);

	if (adminIgnoreRoutes.includes(location.pathname)) {
		return <Outlet />;
	}

	return (
		<div className="flex flex-col h-full">
			<AdminHeader />

			<div className="flex flex-row h-full">
				<div className="border-right h-full pl-2 w-fit overflow-y-auto">
					<HierarchicalSideBar
						sections={adminNavigation}
						visibility={visibility}
					/>
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
