import {
  Outlet,
  createFileRoute,
  redirect,
  useLocation,
} from "@tanstack/react-router";

import { GenericSideBar, ServiceHeader } from "@/components";
import { service_routes } from "@/routes";
import { useAuthStore } from "@/stores/AuthStore";

export const Route = createFileRoute("/service")({
  component: RouteComponent,
});

function RouteComponent() {
  const location = useLocation();

  const hideSidebarPatterns = [/^\/service\/events\/.+/];
  const shouldHideSidebar = hideSidebarPatterns.some((pattern) =>
    pattern.test(location.pathname)
  );
  return (
    <div className="h-full w-full flex flex-col">
      <ServiceHeader />
      <div className="flex flex-row flex-1 min-h-0">
        {!shouldHideSidebar && ( // 👈 在这里判断
          <div className="border-right h-full pl-2 w-fit flex-shrink-0 min-h-0">
            <GenericSideBar routes={service_routes} />
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
