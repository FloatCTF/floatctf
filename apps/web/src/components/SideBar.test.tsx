import { NavigationProvider } from "@/navigation";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
	Outlet,
	RouterProvider,
	createMemoryHistory,
	createRootRoute,
	createRoute,
	createRouter,
} from "@tanstack/react-router";
import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
// @vitest-environment jsdom
// 反馈回路：navlist 点击必须走 SPA 内部导航（左侧 nav 常驻、右侧内容区切换）。
// 若 SideBar 用裸 <a href>（整页刷新），此测试变红：路由状态不会改变。
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { GenericSideBar, type NavRoute } from "./SideBar";

// Mock window.scrollTo for jsdom (TanStack Router scroll restoration)
beforeEach(() => {
	window.scrollTo = vi.fn();
});

afterEach(() => {
	cleanup();
});

const navRoutes: NavRoute[] = [
	{ label: "Instances", path: "/service/instances", icon: <span>i</span> },
	{ label: "Solves", path: "/service/solves", icon: <span>s</span> },
];

function setup() {
	const rootRoute = createRootRoute({
		component: () => (
			<NavigationProvider>
				<div>
					<GenericSideBar routes={navRoutes} />
					<Outlet />
				</div>
			</NavigationProvider>
		),
	});
	// 通配符叶子，让任意内部路径都能匹配，避免导航报错
	const catchAll = createRoute({
		getParentRoute: () => rootRoute,
		path: "*",
		component: () => <div data-testid="content">Content</div>,
	});
	rootRoute.addChildren([catchAll]);
	const queryClient = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	});
	const router = createRouter({
		routeTree: rootRoute,
		history: createMemoryHistory({ initialEntries: ["/service/instances"] }),
	});
	return { router, queryClient };
}

describe("GenericSideBar 导航", () => {
	it("点击导航项触发 SPA 内部导航，而不是整页刷新", async () => {
		const { router, queryClient } = setup();
		render(
			<QueryClientProvider client={queryClient}>
				<RouterProvider router={router} />
			</QueryClientProvider>,
		);

		// router 初始匹配是异步的（pending → idle），先等侧栏渲染出来
		await screen.findByRole("link", { name: "Solves" });
		// 重新查询拿到连接 DOM 的最新节点（React 并发提交期间 findByRole 可能捕获到
		// 已替换的旧节点，旧节点 detached，点击事件不会到达 React 合成事件系统）
		const solvesLink = screen.getByRole("link", { name: "Solves" });
		expect(router.state.location.pathname).toBe("/service/instances");

		// 修复前：NavList.Item 是裸 <a href>，点击触发浏览器整页导航（jsdom 不实现），
		// router 状态不变 → 下面的断言超时失败（RED）。
		fireEvent.click(solvesLink);

		await waitFor(() => {
			expect(router.state.location.pathname).toBe("/service/solves");
		});
		// 页面不应发生整页跳转（jsdom 下路径保持原样，SPA 只改内存历史）
		expect(window.location.pathname).toBe("/");
	});
});
