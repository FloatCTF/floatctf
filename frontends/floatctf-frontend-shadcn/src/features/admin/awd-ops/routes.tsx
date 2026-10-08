/**
 * AWD 运维路由自注册（CONVENTIONS §1）。
 * `shell: "admin"` → 自动被 `RequireAdmin` + 管理端外壳包裹，页面内不再写鉴权判断。
 */

import type { FeatureModule } from "~/features/types";

import { AwdOpsPage } from "./pages";

export const feature: FeatureModule = {
	shell: "admin",
	routes: [{ path: "/admin/events/:eventId/awd", element: <AwdOpsPage /> }],
};
