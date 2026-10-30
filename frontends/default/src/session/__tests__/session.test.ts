// @vitest-environment jsdom
import type { WebSessionStatus } from "@floatctf/sdk";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/**
 * 主站登录态编排：服务端状态是唯一依据；旧版 localStorage JWT 平滑迁移；
 * 只有恰好一个可恢复账号时自动恢复；切换用户清空查询缓存；CSRF 轮换后重试一次。
 */

const webSession = {
	status: vi.fn(),
	migrate: vi.fn(),
	restore: vi.fn(),
	login: vi.fn(),
	logout: vi.fn(),
	select: vi.fn(),
};
const clear = vi.fn();

vi.mock("@/api/client", () => ({ client: { service: { webSession } } }));
vi.mock("@/integrations/tanstack-query/root-provider", () => ({
	getContext: () => ({ queryClient: { clear } }),
}));

function status(partial: Partial<WebSessionStatus> = {}): WebSessionStatus {
	return {
		mode: "cookie",
		authenticated: false,
		user: null,
		csrf_token: "csrf-1",
		accounts: [],
		auto_restore: false,
		max_accounts: 5,
		global_logout: false,
		...partial,
	};
}

const alice = { id: "u-alice", username: "alice", nickname: "Alice" };
const bob = { id: "u-bob", username: "bob", nickname: "Bob" };

async function load() {
	const session = await import("@/session");
	const { useAuthStore } = await import("@/stores/AuthStore");
	const { useSessionStore } = await import("@/stores/SessionStore");
	return { session, useAuthStore, useSessionStore };
}

function installStorage() {
	const data = new Map<string, string>();
	Object.defineProperty(window, "localStorage", {
		configurable: true,
		value: {
			getItem: (k: string) => data.get(k) ?? null,
			setItem: (k: string, v: string) => void data.set(k, String(v)),
			removeItem: (k: string) => void data.delete(k),
			clear: () => data.clear(),
			key: (i: number) => [...data.keys()][i] ?? null,
			get length() {
				return data.size;
			},
		},
	});
}

beforeEach(() => {
	installStorage();
	vi.resetModules();
	for (const fn of Object.values(webSession)) fn.mockReset();
	clear.mockReset();
});

afterEach(() => {
	vi.restoreAllMocks();
});

describe("ensureSession", () => {
	it("uses the legacy JWT when the deployment has not enabled unified sessions", async () => {
		webSession.status.mockResolvedValue(status({ mode: "legacy", csrf_token: null }));
		const { session, useAuthStore } = await load();
		expect(await session.isSignedIn()).toBe(false);
		useAuthStore.getState().setToken("jwt");
		expect(await session.isSignedIn()).toBe(true);
		expect(session.sessionHeaders()).toBeNull();
	});

	it("treats a missing endpoint (older API) as legacy mode", async () => {
		webSession.status.mockRejectedValue({ httpStatus: 404 });
		const { session } = await load();
		const st = await session.ensureSession();
		expect(st.mode).toBe("legacy");
	});

	it("migrates a legacy localStorage token into a server session and then deletes it", async () => {
		webSession.status.mockResolvedValue(status({ csrf_token: null }));
		webSession.migrate.mockResolvedValue(
			status({ authenticated: true, user: alice, csrf_token: "csrf-2" }),
		);
		const { session, useAuthStore } = await load();
		useAuthStore.getState().setToken("old-jwt");
		expect(await session.isSignedIn()).toBe(true);
		expect(webSession.migrate).toHaveBeenCalledWith("old-jwt", null);
		expect(useAuthStore.getState().token).toBeNull();
		expect(session.sessionHeaders()).toEqual({
			"X-CSRF-Token": "csrf-2",
			"X-FloatCTF-Account": "u-alice",
		});
	});

	it("drops a token the server rejects but keeps it when the network fails", async () => {
		webSession.status.mockResolvedValue(status());
		webSession.migrate.mockRejectedValueOnce({ httpStatus: 401 });
		let ctx = await load();
		ctx.useAuthStore.getState().setToken("expired-jwt");
		expect(await ctx.session.isSignedIn()).toBe(false);
		expect(ctx.useAuthStore.getState().token).toBeNull();

		vi.resetModules();
		webSession.migrate.mockRejectedValueOnce(new Error("Network Error"));
		ctx = await load();
		ctx.useAuthStore.getState().setToken("valid-jwt");
		await expect(ctx.session.ensureSession()).rejects.toThrow("Network Error");
		expect(ctx.useAuthStore.getState().token).toBe("valid-jwt");
	});

	it("auto-restores only when the server says exactly one account is restorable", async () => {
		webSession.status.mockResolvedValue(status({ auto_restore: true }));
		webSession.restore.mockResolvedValue(
			status({ authenticated: true, user: alice }),
		);
		const { session } = await load();
		expect(await session.isSignedIn()).toBe(true);
		expect(webSession.restore).toHaveBeenCalledTimes(1);

		vi.resetModules();
		webSession.restore.mockReset();
		webSession.status.mockResolvedValue(
			status({ auto_restore: false, accounts: [] }),
		);
		const second = await load();
		expect(await second.session.isSignedIn()).toBe(false);
		expect(webSession.restore).not.toHaveBeenCalled();
	});
});

describe("account changes", () => {
	it("clears cached queries when the current user changes", async () => {
		const { session } = await load();
		session.applyStatus(status({ authenticated: true, user: alice }));
		expect(clear).not.toHaveBeenCalled();
		session.applyStatus(status({ authenticated: true, user: alice }));
		expect(clear).not.toHaveBeenCalled();
		session.applyStatus(status({ authenticated: true, user: bob }));
		expect(clear).toHaveBeenCalledTimes(1);
		session.applyStatus(status({ authenticated: false, user: null }));
		expect(clear).toHaveBeenCalledTimes(2);
	});

	it("retries once with a fresh CSRF token after another tab rotated it", async () => {
		const { session } = await load();
		session.applyStatus(status({ authenticated: true, user: alice, csrf_token: "stale" }));
		webSession.status.mockResolvedValue(
			status({ authenticated: true, user: alice, csrf_token: "fresh" }),
		);
		const run = vi
			.fn()
			.mockRejectedValueOnce({ httpStatus: 403 })
			.mockResolvedValueOnce(status({ authenticated: false, csrf_token: "fresh" }));
		const next = await session.mutateSession(run);
		expect(run.mock.calls.map((c) => c[0])).toEqual(["stale", "fresh"]);
		expect(next.authenticated).toBe(false);
	});

	it("surfaces a failed sign-out instead of pretending it succeeded", async () => {
		const { session, useSessionStore } = await load();
		session.applyStatus(status({ authenticated: true, user: alice }));
		await expect(
			session.mutateSession(() => Promise.reject({ httpStatus: 500 })),
		).rejects.toEqual({ httpStatus: 500 });
		expect(useSessionStore.getState().status?.authenticated).toBe(true);
	});
});
