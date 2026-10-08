/**
 * 管理端核心域路由（全部 `shell: "admin"` → 自动被 `RequireAdmin` + 管理端外壳包裹）。
 *
 * 路由自注册：`src/app/router.tsx` 用 `import.meta.glob` 收集本文件导出的 `feature`，
 * 因此这里新增/调整路由不需要改动任何共享文件。
 */

import type { FeatureModule } from "~/features/types";

import {
	AdminChallengeSetDetailPage,
	AdminChallengeSetsPage,
	AdminChallengesPage,
	AdminDashboardPage,
	AdminGameboxesPage,
	AdminNetworkPage,
	AdminUsersPage,
} from "./pages";

export const feature: FeatureModule = {
	shell: "admin",
	routes: [
		{ path: "/admin", element: <AdminDashboardPage /> },
		{ path: "/admin/platform/users", element: <AdminUsersPage /> },
		{ path: "/admin/challenges", element: <AdminChallengesPage /> },
		{ path: "/admin/challenge-sets", element: <AdminChallengeSetsPage /> },
		{ path: "/admin/challenge-sets/:setId", element: <AdminChallengeSetDetailPage /> },
		{ path: "/admin/gameboxes", element: <AdminGameboxesPage /> },
		{ path: "/admin/network", element: <AdminNetworkPage /> },
	],
};
