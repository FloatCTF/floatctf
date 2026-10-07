/**
 * Default Frontend 自己的路由实例。
 *
 * 关键点：**路由完全由前端拥有**。`@floatctf/frontend-runtime` 与后端都不知道
 * `/service/events/awd/$id` 这类路径的存在，所以另一个前端可以自由地改用
 * `/workspace/:id`。本文件与迁移前的 `@/main` 中创建 router 的部分逐字一致，
 * 只是从 `main.tsx` 拆出来，好让 `@/api/client` 能在 401 时导航到既有入口。
 */

import { RouterProvider, createRouter } from "@tanstack/react-router";

import * as TanStackQueryProvider from "./integrations/tanstack-query/root-provider.tsx";
import { RouteLoading } from "./components/RouteLoading.tsx";
// 导入生成的路由树
import { routeTree } from "./routeTree.gen";

export const TanStackQueryProviderContext = TanStackQueryProvider.getContext();

export const router = createRouter({
	routeTree,
	context: {
		...TanStackQueryProviderContext,
	},
	defaultPreload: "intent",
	scrollRestoration: true,
	defaultStructuralSharing: true,
	defaultPreloadStaleTime: 0,
	// 懒加载 chunk / loader 期间在内容区显示加载态，替代默认白屏。
	defaultPendingComponent: RouteLoading,
	defaultPendingMs: 100, // 100ms 内完成的路由切换不闪烁加载态
	defaultPendingMinMs: 300, // 加载态至少展示 300ms，避免快速加载时闪一下
	// 路由加载失败 / 404 时给出提示，避免整页白屏。
	defaultErrorComponent: ({ error }: { error: Error }) => (
		<div className="flex h-full w-full items-center justify-center">
			<p className="text-red-600">页面加载失败：{String(error)}</p>
		</div>
	),
	defaultNotFoundComponent: () => (
		<div className="flex h-full w-full items-center justify-center">
			<p>404 · 页面不存在</p>
		</div>
	),
});

// 注册路由实例以获得类型安全
declare module "@tanstack/react-router" {
	interface Register {
		router: typeof router;
	}
}

export { RouterProvider };
