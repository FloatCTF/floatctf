/**
 * AWDP 运维路由自注册（CONVENTIONS §1）。
 * `shell: "admin"` → 自动被 `RequireAdmin` + 管理端外壳包裹。
 */

import type { FeatureModule } from "~/features/types";

import { AwdpOpsPage } from "./pages";

export const feature: FeatureModule = {
	shell: "admin",
	routes: [{ path: "/admin/events/:eventId/awdp", element: <AwdpOpsPage /> }],
};
