/**
 * 社区域路由自注册 —— 全部走选手端外壳（`shell: "player"`）。
 *
 * 路径与 `src/app/nav.ts` 的信息架构一致；`/community/discussions/mine` 是静态段，
 * 与 `/community/discussions/:id` 由 React Router 的排名规则正确区分。
 *
 * 说明：题解（`/writeups`、`/writeups/:id`）在本模块内实现，因为本域只允许改动
 * `src/features/community/**`；它们与讨论 / 公告同属「社区内容」。
 */

import type { FeatureModule } from "~/features/types";

import {
	DiscussionDetailPage,
	DiscussionsPage,
	MyDiscussionsPage,
} from "./discussions";
import { AnnouncementsPage, ArsenalPage, RankPage, SolvesPage } from "./pages";
import { WriteupDetailPage, WriteupsPage } from "./writeups";

export const feature: FeatureModule = {
	shell: "player",
	routes: [
		{ path: "/rank", element: <RankPage /> },
		{ path: "/solves", element: <SolvesPage /> },
		{ path: "/community/announcements", element: <AnnouncementsPage /> },
		{ path: "/community/discussions", element: <DiscussionsPage /> },
		{ path: "/community/discussions/mine", element: <MyDiscussionsPage /> },
		{ path: "/community/discussions/:id", element: <DiscussionDetailPage /> },
		{ path: "/arsenal", element: <ArsenalPage /> },
		{ path: "/writeups", element: <WriteupsPage /> },
		{ path: "/writeups/:id", element: <WriteupDetailPage /> },
	],
};
