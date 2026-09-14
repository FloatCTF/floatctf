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
	component: RouteComponent,
	// loader: AdminRouteGuard,
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
