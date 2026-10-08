/**
 * 管理端 · 基础设施路由（自注册，`shell: "admin"`）。
 *
 * `/admin/version` 在本前端被归入「基础设施」导航组（见 `src/app/nav.ts`）。
 */

import type { FeatureModule } from "~/features/types";

import {
	AdminDockerPage,
	AdminSqlPage,
	AdminTerminalPage,
	AdminVersionPage,
} from "./pages";

export const feature: FeatureModule = {
	shell: "admin",
	routes: [
		{ path: "/admin/infra/docker", element: <AdminDockerPage /> },
		{ path: "/admin/infra/sql", element: <AdminSqlPage /> },
		{ path: "/admin/infra/terminal", element: <AdminTerminalPage /> },
		{ path: "/admin/version", element: <AdminVersionPage /> },
	],
};
