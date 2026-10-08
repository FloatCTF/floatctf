/**
 * 管理端 · 社区治理路由（自注册，`shell: "admin"`）。
 *
 * 导航项已存在于 `src/app/nav.ts`（社区 → 公告 / 讨论 / 武器库），此处只挂路由。
 */

import type { FeatureModule } from "~/features/types";

import {
	AdminAnnouncementsPage,
	AdminDiscussionsPage,
	AdminWeaponsPage,
} from "./pages";

export const feature: FeatureModule = {
	shell: "admin",
	routes: [
		{ path: "/admin/community/announcements", element: <AdminAnnouncementsPage /> },
		{ path: "/admin/community/discussions", element: <AdminDiscussionsPage /> },
		{ path: "/admin/community/weapons", element: <AdminWeaponsPage /> },
	],
};
