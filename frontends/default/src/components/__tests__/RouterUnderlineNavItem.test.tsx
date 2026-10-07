// @vitest-environment jsdom
import { ThemeProvider, UnderlineNav } from "@primer/react";
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
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { NavigationProvider } from "@/navigation";
import { RouterUnderlineNavItem } from "../RouterUnderlineNavItem";

beforeEach(() => {
	window.scrollTo = vi.fn();
});

afterEach(() => {
	cleanup();
});

function setup() {
	const rootRoute = createRootRoute({
		component: () => (
			<NavigationProvider>
				<Outlet />
			</NavigationProvider>
		),
	});
	const indexRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/",
		component: () => (
			<UnderlineNav aria-label="Test navigation">
				<RouterUnderlineNavItem to="/">Overview</RouterUnderlineNavItem>
				<RouterUnderlineNavItem to="/images">Images</RouterUnderlineNavItem>
			</UnderlineNav>
		),
	});
	const imagesRoute = createRoute({
		getParentRoute: () => rootRoute,
		path: "/images",
		component: () => <div>Images page</div>,
	});
	rootRoute.addChildren([indexRoute, imagesRoute]);

	const router = createRouter({
		routeTree: rootRoute,
		history: createMemoryHistory({ initialEntries: ["/"] }),
		defaultPreload: "intent",
	});
	const queryClient = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	});

	const result = render(
		<QueryClientProvider client={queryClient}>
			<ThemeProvider>
				<RouterProvider router={router} />
			</ThemeProvider>
		</QueryClientProvider>,
	);
	return { router, ...result };
}

describe("RouterUnderlineNavItem", () => {
	it("renders one semantic anchor per nav item with the real route href", async () => {
		const { container } = setup();

		const images = await screen.findByRole("link", { name: "Images" });
		expect(images.getAttribute("href")).toBe("/images");
		expect(container.querySelectorAll("a")).toHaveLength(2);
		expect(container.querySelectorAll("a a")).toHaveLength(0);
		expect(
			screen
				.getByRole("link", { name: "Overview" })
				.getAttribute("aria-current"),
		).toBe("page");
	});

	it("navigates through the AppLink coordinator", async () => {
		const { router } = setup();
		fireEvent.click(await screen.findByRole("link", { name: "Images" }));

		await waitFor(() => {
			expect(router.state.location.pathname).toBe("/images");
		});
		await screen.findByText("Images page");
	});
});
