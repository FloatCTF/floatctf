/**
 * 路由表 —— 由 feature 模块**自注册**（`src/features/<域>/routes.tsx`）+ 三个外壳组成。
 * React Router v7 自动做路由排名，因此静态段与参数段的书写顺序不影响匹配。
 */

import { createBrowserRouter, type RouteObject } from "react-router";

import type { FeatureModule } from "~/features/types";

import { RequireAdmin, RequireUser } from "./guards";
import { NotFoundPage } from "./not-found";
import { RootLayout } from "./providers";
import { PublicShell } from "./public-shell";
import { AdminShell, PlayerShell } from "./shell";

const featureModules = import.meta.glob<{ feature?: FeatureModule }>(
	"../features/**/routes.tsx",
	{ eager: true },
);

function collectRoutes(shell: FeatureModule["shell"]): RouteObject[] {
	const routes: RouteObject[] = [];
	for (const module of Object.values(featureModules)) {
		const feature = module.feature;
		if (!feature || feature.shell !== shell) continue;
		routes.push(...feature.routes);
	}
	return routes;
}

export function createAppRouter() {
	return createBrowserRouter([
		{
			element: <RootLayout />,
			children: [
				{
					element: <PublicShell />,
					children: collectRoutes("public"),
				},
				{
					element: (
						<RequireUser>
							<PlayerShell />
						</RequireUser>
					),
					children: collectRoutes("player"),
				},
				{
					element: (
						<RequireAdmin>
							<AdminShell />
						</RequireAdmin>
					),
					children: collectRoutes("admin"),
				},
				{ path: "*", element: <NotFoundPage /> },
			],
		},
	]);
}
