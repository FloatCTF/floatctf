/**
 * 个人资料路由自注册（选手端外壳）。
 */

import type { FeatureModule } from "~/features/types";

import { ProfilePage } from "./pages";

export const feature: FeatureModule = {
	shell: "player",
	routes: [{ path: "/profile", element: <ProfilePage /> }],
};
