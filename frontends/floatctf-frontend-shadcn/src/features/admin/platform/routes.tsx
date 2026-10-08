/**
 * 管理端 · 平台治理路由（自注册，`shell: "admin"`）。
 *
 * 注意：`/admin/platform/users` 不属于本域（由 `features/admin/core` 负责），此处不注册。
 */

import type { FeatureModule } from "~/features/types";

import {
	AdminFrontendsPage,
	AdminLogsPage,
	AdminScheduledTasksPage,
	AdminSettingsPage,
	AdminSuperAdminsPage,
} from "./pages";

export const feature: FeatureModule = {
	shell: "admin",
	routes: [
		{ path: "/admin/platform/super-admins", element: <AdminSuperAdminsPage /> },
		{ path: "/admin/platform/logs", element: <AdminLogsPage /> },
		{ path: "/admin/platform/settings", element: <AdminSettingsPage /> },
		{ path: "/admin/platform/frontends", element: <AdminFrontendsPage /> },
		{ path: "/admin/platform/tasks", element: <AdminScheduledTasksPage /> },
	],
};
