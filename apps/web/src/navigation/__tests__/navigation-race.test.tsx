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

// Mock window.scrollTo for jsdom (TanStack Router scroll restoration)
beforeEach(() => {
	window.scrollTo = vi.fn();
});

afterEach(() => {
	cleanup();
});

// ── Test helper ───────────────────────────────────────────────────────────────

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

	const slowRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/slow",
		component: () => <div data-testid="slow">Slow Page</div>,
		loader: () => new Promise((resolve) => setTimeout(resolve, 2000)), // 2s delay
	});

	const fastRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/fast",
		component: () => <div data-testid="fast">Fast Page</div>,
	});

	const aboutRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/about",
		component: () => <div data-testid="about">About</div>,
	});

	rootRoute.addChildren([indexRoute, slowRoute, fastRoute, aboutRoute]);

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

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("Navigation transaction race", () => {
	it("later navigation supersedes earlier one", async () => {
		const { router } = setup(
			"/",
			<div>
				<AppLink to="/slow">Slow</AppLink>
				<AppLink to="/fast">Fast</AppLink>
			</div>,
		);

		const slowLink = await screen.findByText("Slow");
		const fastLink = await screen.findByText("Fast");

		// Click slow first (will take 2s to load)
		fireEvent.click(slowLink);

		// Immediately click fast (should supersede)
		fireEvent.click(fastLink);

		// The fast route should win — user sees fast page
		await waitFor(
			() => {
				expect(screen.queryByTestId("fast")).toBeTruthy();
			},
			{ timeout: 5000 },
		);

		// Should NOT end up on slow page
		expect(router.state.location.pathname).toBe("/fast");
	});
});

describe("Navigation progress cleanup", () => {
	it("progress bar resets after navigation completes", async () => {
		const { router } = setup("/", <AppLink to="/about">Go</AppLink>);

		const link = await screen.findByText("Go");
		fireEvent.click(link);

		await waitFor(() => {
			expect(router.state.location.pathname).toBe("/about");
		});

		// Progress bar should be hidden (opacity 0 or not visible)
		await waitFor(
			() => {
				const bar = document.querySelector(".floatctf-nav-progress");
				if (bar) {
					const style = window.getComputedStyle(bar);
					expect(Number(style.opacity)).toBe(0);
				}
			},
			{ timeout: 2000 },
		);
	});
});
