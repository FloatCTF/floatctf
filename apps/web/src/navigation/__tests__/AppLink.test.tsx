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
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AppLink } from "../AppLink";
import { NavigationProvider } from "../NavigationContext";

// 为 jsdom mock window.scrollTo（TanStack Router 滚动恢复）
beforeEach(() => {
	window.scrollTo = vi.fn();
});

afterEach(() => {
	cleanup();
});

// ── 测试辅助 ───────────────────────────────────────────────────────────────

function setup(initialPath: string, indexContent: React.ReactNode) {
	const rootRoute = createRootRoute({
		component: () => (
			<NavigationProvider>
				<div data-testid="root">
					<Outlet />
				</div>
			</NavigationProvider>
		),
	});

	const indexRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/",
		component: () => <>{indexContent}</>,
	});

	const aboutRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/about",
		component: () => <div data-testid="about">About</div>,
	});

	const settingsRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/settings",
		component: () => <div data-testid="settings">Settings</div>,
	});

	rootRoute.addChildren([indexRoute, aboutRoute, settingsRoute]);

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

// ── 测试 ─────────────────────────────────────────────────────────────────────

describe("AppLink", () => {
	it("navigates on plain left click via coordinator", async () => {
		const { router } = setup("/", <AppLink to="/about">Go About</AppLink>);

		const link = await screen.findByText("Go About");
		expect(router.state.location.pathname).toBe("/");

		fireEvent.click(link);

		await waitFor(() => {
			expect(router.state.location.pathname).toBe("/about");
		});
	});

	it("does NOT navigate on Ctrl+click (opens new tab)", async () => {
		const { router } = setup("/", <AppLink to="/about">Go About</AppLink>);
		const navigateSpy = vi.spyOn(router, "navigate");

		const link = await screen.findByText("Go About");
		fireEvent.click(link, { ctrlKey: true });

		expect(router.state.location.pathname).toBe("/");
		expect(navigateSpy).not.toHaveBeenCalled();
	});

	it("does NOT navigate on Meta+click", async () => {
		const { router } = setup("/", <AppLink to="/about">Go About</AppLink>);
		const navigateSpy = vi.spyOn(router, "navigate");

		const link = await screen.findByText("Go About");
		fireEvent.click(link, { metaKey: true });

		expect(router.state.location.pathname).toBe("/");
		expect(navigateSpy).not.toHaveBeenCalled();
	});

	it("does NOT navigate on Shift+click", async () => {
		const { router } = setup("/", <AppLink to="/about">Go About</AppLink>);
		const navigateSpy = vi.spyOn(router, "navigate");

		const link = await screen.findByText("Go About");
		fireEvent.click(link, { shiftKey: true });

		expect(router.state.location.pathname).toBe("/");
		expect(navigateSpy).not.toHaveBeenCalled();
	});

	it("does NOT navigate on middle click (button 1)", async () => {
		const { router } = setup("/", <AppLink to="/about">Go About</AppLink>);
		const navigateSpy = vi.spyOn(router, "navigate");

		const link = await screen.findByText("Go About");
		fireEvent.click(link, { button: 1 });

		expect(router.state.location.pathname).toBe("/");
		expect(navigateSpy).not.toHaveBeenCalled();
	});

	it("bypasses coordinator when target=_blank", async () => {
		const { router } = setup(
			"/",
			<AppLink to="/about" target="_blank">
				External
			</AppLink>,
		);
		const navigateSpy = vi.spyOn(router, "navigate");

		const link = await screen.findByText("External");
		fireEvent.click(link);

		expect(router.state.location.pathname).toBe("/");
		expect(navigateSpy).not.toHaveBeenCalled();
	});

	it("bypasses coordinator for external URL to= (https://)", async () => {
		const { router } = setup(
			"/",
			<AppLink to="https://example.com">ExtLink</AppLink>,
		);
		const navigateSpy = vi.spyOn(router, "navigate");

		const link = await screen.findByText("ExtLink");
		fireEvent.click(link);

		expect(router.state.location.pathname).toBe("/");
		expect(navigateSpy).not.toHaveBeenCalled();
	});

	it("bypasses coordinator when download is set", async () => {
		const { router } = setup(
			"/",
			<AppLink to="/file.pdf" download="file.pdf">
				Download
			</AppLink>,
		);
		const navigateSpy = vi.spyOn(router, "navigate");

		const link = await screen.findByText("Download");
		fireEvent.click(link);

		expect(router.state.location.pathname).toBe("/");
		expect(navigateSpy).not.toHaveBeenCalled();
	});

	it("calls consumer onClick before navigation", async () => {
		const onClick = vi.fn();
		const { router } = setup(
			"/",
			<AppLink to="/about" onClick={onClick}>
				Click Me
			</AppLink>,
		);

		const link = await screen.findByText("Click Me");
		fireEvent.click(link);

		expect(onClick).toHaveBeenCalledTimes(1);
		await waitFor(() => {
			expect(router.state.location.pathname).toBe("/about");
		});
	});

	it("renders as a link with href", async () => {
		setup("/", <AppLink to="/about">About Page</AppLink>);

		const link = await screen.findByRole("link", { name: "About Page" });
		expect(link.getAttribute("href")).toBe("/about");
	});
});
