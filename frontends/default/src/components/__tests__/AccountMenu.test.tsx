// @vitest-environment jsdom
import type { WebSessionStatus } from "@floatctf/sdk";
import { BaseStyles, ThemeProvider } from "@primer/react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useSessionStore } from "@/stores/SessionStore";
import { AccountAvatar, AccountMenu, GlobalLogoutNotice } from "../AccountMenu";

/**
 * 主站账号菜单：只列服务端给出的本浏览器有效账号；切换 / 退出都等服务端确认；
 * 失败如实提示且登录状态不变。
 */

const { navigate, webSession } = vi.hoisted(() => ({
	navigate: vi.fn(),
	webSession: { select: vi.fn(), logout: vi.fn(), status: vi.fn() },
}));

vi.mock("@/navigation", () => ({ useAppNavigate: () => navigate }));
vi.mock("@/api", () => ({ client: { service: { webSession } } }));
vi.mock("@/api/client", () => ({ client: { service: { webSession } } }));
vi.mock("@/integrations/tanstack-query/root-provider", () => ({
	getContext: () => ({ queryClient: { clear: vi.fn() } }),
}));

const alice = { id: "u-a", username: "alice", nickname: "Alice 同学" };
const bob = { id: "u-b", username: "bob", nickname: "Bob 同学" };

function status(user = alice): WebSessionStatus {
	return {
		mode: "cookie",
		authenticated: true,
		user,
		csrf_token: "csrf",
		accounts: [
			{ id: "s-a", user_id: alice.id, username: "alice", nickname: alice.nickname, active: user.id === alice.id },
			{ id: "s-b", user_id: bob.id, username: "bob", nickname: bob.nickname, active: user.id === bob.id },
		],
		auto_restore: false,
		max_accounts: 5,
		global_logout: false,
	};
}

async function setup(globalLogout = false) {
	useSessionStore.getState().setStatus({ ...status(), global_logout: globalLogout });
	render(
		<ThemeProvider>
			<BaseStyles>
				<AccountMenu />
			</BaseStyles>
		</ThemeProvider>,
	);
	fireEvent.click(screen.getByRole("button", { name: /账号菜单/ }));
	return { useSessionStore };
}

beforeEach(() => {
	globalThis.ResizeObserver ??= class {
		observe() {}
		unobserve() {}
		disconnect() {}
	} as unknown as typeof ResizeObserver;
	window.matchMedia = ((query: string) => ({
		matches: false,
		media: query,
		onchange: null,
		addListener: () => undefined,
		removeListener: () => undefined,
		addEventListener: () => undefined,
		removeEventListener: () => undefined,
		dispatchEvent: () => false,
	})) as unknown as typeof window.matchMedia;
	const data = new Map<string, string>();
	Object.defineProperty(window, "localStorage", {
		configurable: true,
		value: {
			getItem: (k: string) => data.get(k) ?? null,
			setItem: (k: string, v: string) => void data.set(k, v),
			removeItem: (k: string) => void data.delete(k),
			clear: () => data.clear(),
			key: () => null,
			length: 0,
		},
	});
	navigate.mockReset();
	for (const fn of Object.values(webSession)) fn.mockReset();
});

afterEach(() => cleanup());

describe("AccountMenu", () => {
	it("switches to another account in this browser after the server confirms", async () => {
		webSession.select.mockResolvedValue(status(bob));
		const { useSessionStore } = await setup();
		fireEvent.click(await screen.findByText("Bob 同学"));
		await waitFor(() => expect(webSession.select).toHaveBeenCalledWith("s-b", "csrf"));
		await waitFor(() => expect(navigate).toHaveBeenCalledWith({ to: "/service/top" }));
		expect(useSessionStore.getState().status?.user?.id).toBe(bob.id);
	});

	it("keeps the user signed in and explains when sign-out fails", async () => {
		webSession.logout.mockRejectedValue({ httpStatus: 500, response: { status: 500, data: {} } });
		const { useSessionStore } = await setup();
		fireEvent.click(await screen.findByText("退出当前账号"));
		expect(await screen.findByText("退出未完成")).toBeTruthy();
		expect(navigate).not.toHaveBeenCalled();
		expect(useSessionStore.getState().status?.authenticated).toBe(true);
	});

	it("reports every relying party honestly after signing out of all linked apps, even after the menu unmounts", async () => {
		webSession.logout.mockResolvedValue({
			...status(),
			authenticated: false,
			user: null,
			accounts: [],
			logout_report: {
				deliveries: [
					{ client_id: "c1", client_name: "LearnOS", status: "delivered" },
					{ client_id: "c2", client_name: "Wiki", status: "pending", detail: "无法连接" },
				],
				without_backchannel: ["Legacy App"],
			},
		});
		useSessionStore.getState().setLogoutReport(null);
		useSessionStore.getState().setStatus({ ...status(), global_logout: true });
		// 与真实应用一致：菜单在 /service 布局里，结果对话框挂在根布局。
		const tree = (withMenu: boolean) => (
			<ThemeProvider>
				<BaseStyles>
					{withMenu && <AccountMenu />}
					<GlobalLogoutNotice />
				</BaseStyles>
			</ThemeProvider>
		);
		const view = render(tree(true));
		fireEvent.click(screen.getByRole("button", { name: /账号菜单/ }));
		fireEvent.click(await screen.findByText("退出所有关联应用…"));
		// 确认对话框
		fireEvent.click(await screen.findByRole("button", { name: "退出所有关联应用" }));
		await waitFor(() => expect(webSession.logout).toHaveBeenCalledWith("global", null, "csrf"));
		// 退出后路由离开 /service：账号菜单被卸载，结果仍然要可见
		view.rerender(tree(false));
		expect(await screen.findByText("已退出 FloatCTF，但应用未全部同步")).toBeTruthy();
		expect(screen.getByText("已确认退出")).toBeTruthy();
		expect(screen.getByText("未送达，将自动重试")).toBeTruthy();
		expect(screen.getByText(/Legacy App/)).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "完成" }));
		await waitFor(() => expect(screen.queryByText("已退出 FloatCTF，但应用未全部同步")).toBeNull());
		expect(navigate).toHaveBeenCalledWith({ to: "/" });
		expect(useSessionStore.getState().logoutReport).toBeNull();
	});

	it("hides the global sign-out when the deployment does not support it", async () => {
		await setup(false);
		expect(await screen.findByText("退出当前账号")).toBeTruthy();
		expect(screen.queryByText("退出所有关联应用…")).toBeNull();
	});

	it("offers adding an account and signing out of every account in this browser", async () => {
		await setup();
		expect(await screen.findByText("添加账号")).toBeTruthy();
		expect(screen.getByText("退出本浏览器中的全部账号")).toBeTruthy();
		expect(screen.getByText("只退出 FloatCTF，不影响已登录的第三方应用")).toBeTruthy();
	});

	it("falls back to the initial instead of a broken image when the avatar cannot be loaded", () => {
		const { container } = render(
			<ThemeProvider>
				<AccountAvatar account={{ avatar: "/public/images/missing.png", nickname: "Alice 同学", username: "alice" }} />
			</ThemeProvider>,
		);
		const img = container.querySelector("img");
		expect(img?.getAttribute("src")).toBe("/public/images/missing.png");
		fireEvent.error(img!);
		expect(container.querySelector("img")).toBeNull();
		expect(container.querySelector("[data-avatar-fallback]")?.textContent).toBe("A");
	});

	it("never renders avatars outside the platform's public storage", () => {
		const { container } = render(
			<ThemeProvider>
				<AccountAvatar account={{ avatar: "https://evil.example/x.png", nickname: "", username: "bob" }} />
			</ThemeProvider>,
		);
		expect(container.querySelector("img")).toBeNull();
		expect(container.textContent).toBe("B");
	});
});
