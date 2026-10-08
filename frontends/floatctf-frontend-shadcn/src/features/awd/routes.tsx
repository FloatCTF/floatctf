import type { FeatureModule } from "~/features/types";

import { ArenaPage } from "./pages";

/**
 * AWD 驾驶舱 —— 单一工作区（`/arena/:eventId`），不是 Default 的 overview /
 * gameboxes / scoreboard / wireguard / ssh 多页模型。
 */
export const feature: FeatureModule = {
	shell: "player",
	routes: [{ path: "/arena/:eventId", element: <ArenaPage /> }],
};
