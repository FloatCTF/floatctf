import type { FeatureModule } from "~/features/types";

import { InstancesPage } from "./pages";

export const feature: FeatureModule = {
	shell: "player",
	routes: [{ path: "/instances", element: <InstancesPage /> }],
};
