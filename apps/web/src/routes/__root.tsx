import { TanstackDevtools } from "@tanstack/react-devtools";
import { Outlet, createRootRouteWithContext } from "@tanstack/react-router";
import { TanStackRouterDevtoolsPanel } from "@tanstack/react-router-devtools";

import TanStackQueryDevtools from "../integrations/tanstack-query/devtools";
import { NavigationProgress, NavigationProvider } from "../navigation";

import type { QueryClient } from "@tanstack/react-query";
const showDevtools = import.meta.env.VITE_SHOW_DEVTOOLS === "true";
interface MyRouterContext {
	queryClient: QueryClient;
}

export const Route = createRootRouteWithContext<MyRouterContext>()({
	component: () => (
		<NavigationProvider>
			<div className="h-screen">
				<NavigationProgress />
				<Outlet />
				{showDevtools && (
					<TanstackDevtools
						config={{
							position: "bottom-left",
						}}
						plugins={[
							{
								name: "Tanstack Router",
								render: <TanStackRouterDevtoolsPanel />,
							},
							TanStackQueryDevtools,
						]}
					/>
				)}
			</div>
		</NavigationProvider>
	),
});
