import type { FeatureModule } from "~/features/types";

import { LabPage } from "./pages";

/**
 * AWDP 比赛工作台 —— 阶段驱动的单页工作区（`/lab/:eventId`）：
 * 顶部阶段步进条 + 左「我的靶机」+ 中「Break / Fix 动作区」+ 右「轮次与评测」
 * + 底部「积分榜 / 我的积分 / 趋势」Tabs。
 */
export const feature: FeatureModule = {
	shell: "player",
	routes: [{ path: "/lab/:eventId", element: <LabPage /> }],
};
