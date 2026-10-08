/**
 * 管理端赛事功能模块 —— 自注册到 admin shell（`RequireAdmin` + `AdminShell`）。
 *
 * 一个赛事 = 一个控制台：`/admin/events/:eventId` 内以标签面板承载各域
 * （配置 / 题目 / 成员 / 战队 / 公告 / 实例 / 日志 / Writeup / 数据大屏 / AWD / AWDP）。
 */

import type { FeatureModule } from "~/features/types";

import { EventConsolePage } from "./console";
import { EventsListPage } from "./pages";

export const feature: FeatureModule = {
	shell: "admin",
	routes: [
		{ path: "/admin/events", element: <EventsListPage /> },
		{ path: "/admin/events/:eventId", element: <EventConsolePage /> },
	],
};
