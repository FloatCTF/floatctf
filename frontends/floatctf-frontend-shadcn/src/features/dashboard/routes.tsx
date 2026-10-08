import type { FeatureModule } from "~/features/types";

import { DashboardPage } from "./pages";

export const feature: FeatureModule = {
	shell: "player",
	routes: [{ path: "/", element: <DashboardPage /> }],
};
