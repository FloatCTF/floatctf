/**
 * Feature 路由契约 —— 每个功能模块通过 `src/features/<域>/routes.tsx` 自注册：
 *
 * ```tsx
 * import type { FeatureModule } from "~/features/types";
 * export const feature: FeatureModule = {
 *   shell: "player",
 *   routes: [{ path: "/events", element: <EventsPage /> }],
 * };
 * ```
 *
 * `src/app/router.tsx` 用 `import.meta.glob` 收集全部模块并按 shell 分组，
 * 因此新增页面**不需要**改动路由聚合文件（避免多人并行时的冲突）。
 */

import type { RouteObject } from "react-router";

export type FeatureShell = "public" | "player" | "admin";

export interface FeatureModule {
	shell: FeatureShell;
	/** 路径必须是 root-absolute（`/...`），因为它是无 path 布局路由的子路由。 */
	routes: RouteObject[];
}
