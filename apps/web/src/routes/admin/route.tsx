import {
  Outlet,
  createFileRoute,
  redirect,
  useLocation,
} from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { AdminHeader, GenericSideBar } from "@/components";
import { admin_ignore_routes, admin_routes } from "@/routes";
import { useAuthStore } from "@/stores/AuthStore";

export const Route = createFileRoute("/admin")({
  component: RouteComponent,
  // loader: AdminRouteGuard,
});

function RouteComponent() {
  useTitle("Admin | FloatCTF");
  const location = useLocation();
  if (admin_ignore_routes.includes(location.pathname)) {
    return <Outlet />;
  }

  return (
    <div className="flex flex-col h-full">
      <AdminHeader />

      <div className="flex flex-row  h-full ">
        <div className="border-right h-full  pl-2 w-fit">
          <GenericSideBar routes={admin_routes} />
        </div>
        <div className="p-2 w-full flex-1">
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
