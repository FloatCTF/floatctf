// @vitest-environment jsdom
import { ThemeProvider, UnderlineNav } from "@primer/react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { UnderlineNavButton } from "../UnderlineNavButton";

afterEach(cleanup);

describe("UnderlineNavButton", () => {
	it("renders state-only tabs as buttons without synthetic hash links", () => {
		const { container } = render(
			<ThemeProvider>
				<UnderlineNav aria-label="View">
					<UnderlineNavButton current onClick={() => {}}>
						Current
					</UnderlineNavButton>
					<UnderlineNavButton onClick={() => {}}>Other</UnderlineNavButton>
				</UnderlineNav>
			</ThemeProvider>,
		);

		expect(screen.getByRole("button", { name: "Current" })).toBeDefined();
		expect(screen.getByRole("button", { name: "Other" })).toBeDefined();
		expect(container.querySelectorAll('a[href="#"]')).toHaveLength(0);
		expect(container.querySelectorAll("button")).toHaveLength(2);
		expect(
			screen
				.getByRole("button", { name: "Current" })
				.getAttribute("aria-current"),
		).toBe("page");
	});

	it("invokes the state transition callback", () => {
		const onClick = vi.fn();
		render(
			<ThemeProvider>
				<UnderlineNav aria-label="View">
					<UnderlineNavButton onClick={onClick}>Switch</UnderlineNavButton>
				</UnderlineNav>
			</ThemeProvider>,
		);

		fireEvent.click(screen.getByRole("button", { name: "Switch" }));
		expect(onClick).toHaveBeenCalledTimes(1);
	});
});
