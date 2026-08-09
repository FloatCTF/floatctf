import { NavigationProvider } from "@/navigation";
import type { NavigationSection } from "@/navigation/sidebar-types";
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
// 反馈回路：sidebar 叶子点击必须走 SPA 内部导航（左侧 nav 常驻、右侧内容区切换），
// 且 group 是 button（toggle），active leaf 有 aria-current，active ancestor 自动展开。
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { HierarchicalSideBar } from "./SideBar";

const STORAGE_KEY = "floatctf.admin.navigation.expanded.test";

const sections: NavigationSection[] = [
	{
		id: "section-one",
		label: "Section One",
		children: [
			{
				type: "group",
				id: "group-one",
				label: "Group One",
				children: [
					{
						type: "item",
						id: "group-one.alpha",
						label: "Alpha",
						href: "/admin/alpha",
						match: { mode: "exact" },
					},
					{
						type: "item",
						id: "group-one.beta",
						label: "Beta",
						href: "/admin/beta",
						match: { mode: "exact" },
					},
				],
			},
			{
				type: "item",
				id: "solo",
				label: "Solo",
				href: "/admin/solo",
				match: { mode: "exact" },
			},
		],
	},
];

// Mock window.scrollTo for jsdom (TanStack Router scroll restoration)
beforeEach(() => {
	window.scrollTo = vi.fn();
	// jsdom 无 URL 时 localStorage 不可用：安装内存 shim 供持久化测试使用
	if (typeof window.localStorage === "undefined") {
		const store = new Map<string, string>();
		Object.defineProperty(window, "localStorage", {
			configurable: true,
			value: {
				getItem: (key: string) => store.get(key) ?? null,
				setItem: (key: string, value: string) => {
					store.set(key, value);
				},
				removeItem: (key: string) => {
					store.delete(key);
				},
				clear: () => {
					store.clear();
				},
				key: (index: number) => [...store.keys()][index] ?? null,
				get length() {
					return store.size;
				},
			},
		});
	}
	window.localStorage.clear();
});

afterEach(() => {
	cleanup();
});

function setup(initialPath: string) {
	const rootRoute = createRootRoute({
		component: () => (
			<NavigationProvider>
				<div>
					<HierarchicalSideBar sections={sections} storageKey={STORAGE_KEY} />
					<Outlet />
				</div>
			</NavigationProvider>
		),
	});

	const alphaRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/admin/alpha",
		component: () => <div data-testid="content">Alpha</div>,
	});
	const betaRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/admin/beta",
		component: () => <div data-testid="content">Beta</div>,
	});
	const soloRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/admin/solo",
		component: () => <div data-testid="content">Solo</div>,
	});
	rootRoute.addChildren([alphaRoute, betaRoute, soloRoute]);

	const queryClient = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	});

	const router = createRouter({
		routeTree: rootRoute,
		history: createMemoryHistory({ initialEntries: [initialPath] }),
		defaultPreload: "intent",
	});

	const result = render(
		<QueryClientProvider client={queryClient}>
			<RouterProvider router={router} />
		</QueryClientProvider>,
	);

	return { router, queryClient, ...result };
}

describe("HierarchicalSideBar", () => {
	it("group is a button that toggles its children via aria-expanded", async () => {
		setup("/admin/solo");

		const groupButton = await screen.findByRole("button", {
			name: "Group One",
		});
		expect(groupButton.getAttribute("aria-expanded")).toBe("false");
		expect(screen.queryByRole("link", { name: "Alpha" })).toBeNull();

		fireEvent.click(groupButton);
		await waitFor(() => {
			expect(
				screen
					.getByRole("button", { name: "Group One" })
					.getAttribute("aria-expanded"),
			).toBe("true");
		});
		expect(await screen.findByRole("link", { name: "Alpha" })).toBeTruthy();

		const expandedButton = screen.getByRole("button", { name: "Group One" });
		fireEvent.click(expandedButton);
		await waitFor(() => {
			expect(
				screen
					.getByRole("button", { name: "Group One" })
					.getAttribute("aria-expanded"),
			).toBe("false");
		});
		expect(screen.queryByRole("link", { name: "Alpha" })).toBeNull();
	});

	it("active descendant auto-expands every ancestor and marks the leaf", async () => {
		setup("/admin/beta");

		const groupButton = await screen.findByRole("button", {
			name: "Group One",
		});
		expect(groupButton.getAttribute("aria-expanded")).toBe("true");
		// 祖先只展开，不应出现双重 active（无 aria-current）
		expect(groupButton.getAttribute("aria-current")).toBeNull();
		const betaLink = await screen.findByRole("link", { name: "Beta" });
		expect(betaLink.getAttribute("aria-current")).toBe("page");
		const alphaLink = screen.getByRole("link", { name: "Alpha" });
		expect(alphaLink.getAttribute("aria-current")).toBeNull();
	});

	it("leaf click performs SPA navigation, not a full page reload", async () => {
		const { router } = setup("/admin/alpha");
		await screen.findByRole("link", { name: "Solo" });

		const soloLink = screen.getByRole("link", { name: "Solo" });
		expect(router.state.location.pathname).toBe("/admin/alpha");
		expect(soloLink.getAttribute("href")).toBe("/admin/solo");

		fireEvent.click(soloLink);

		await waitFor(() => {
			expect(router.state.location.pathname).toBe("/admin/solo");
		});
		expect(window.location.pathname).toBe("/");
	});

	it("persists manual expansion across remounts via localStorage", async () => {
		const first = setup("/admin/solo");
		const groupButton = await screen.findByRole("button", {
			name: "Group One",
		});
		fireEvent.click(groupButton);
		await waitFor(() => {
			expect(
				screen
					.getByRole("button", { name: "Group One" })
					.getAttribute("aria-expanded"),
			).toBe("true");
		});
		first.unmount();
		cleanup();

		// 新挂载读同一 localStorage key
		setup("/admin/solo");
		const restoredButton = await screen.findByRole("button", {
			name: "Group One",
		});
		expect(restoredButton.getAttribute("aria-expanded")).toBe("true");
		expect(await screen.findByRole("link", { name: "Alpha" })).toBeTruthy();
	});
});
