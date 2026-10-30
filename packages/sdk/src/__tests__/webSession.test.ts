import { describe, expect, it, vi } from "vitest";

import { createFloatCTFClient } from "../client";
import { connectSse } from "../sse/connectSse";

/**
 * 主站统一 Cookie 会话的 SDK 契约：
 * - 会话头存在时选手端请求**不带** Bearer（服务端 Bearer 优先，混用会绕开 Cookie 通道的 CSRF 校验）；
 * - 管理端永远只走管理员 Bearer；
 * - SSE 每次（重）连接都重新读取会话头；
 * - `webSession.*` 打到 `/users/web-session*`，失败如实抛出。
 */

type Seen = { url?: string; method?: string; headers: Record<string, unknown>; data?: unknown };

function capture(
	instance: { defaults: { adapter?: unknown } },
	seen: Seen[],
	respond: (url?: string) => { status: number; data?: unknown } = () => ({
		status: 200,
		data: { code: 0, data: { mode: "cookie" } },
	}),
) {
	instance.defaults.adapter = async (config: Record<string, unknown>) => {
		const headers = config.headers as { toJSON?: () => Record<string, unknown> };
		seen.push({
			url: config.url as string,
			method: config.method as string,
			headers: headers.toJSON ? headers.toJSON() : (config.headers as Record<string, unknown>),
			data: config.data,
		});
		const { status, data } = respond(config.url as string);
		if (status >= 400) {
			throw Object.assign(new Error(`status ${status}`), {
				isAxiosError: true,
				response: { status, data, statusText: "", headers: {} },
				config,
			});
		}
		return { status, data, statusText: "", headers: {}, config };
	};
}

describe("cookie session transport", () => {
	it("sends session headers instead of a bearer token on the player transport", async () => {
		const seen: Seen[] = [];
		const client = createFloatCTFClient({
			getUserToken: () => "legacy-jwt",
			getAdminToken: () => "admin-jwt",
			getUserSessionHeaders: () => ({
				"X-CSRF-Token": "csrf-1",
				"X-FloatCTF-Account": "user-1",
			}),
		});
		capture(client.transport.service, seen);
		capture(client.transport.admin, seen);

		await client.serviceHttp.get("/users/me");
		await client.adminHttp.get("/settings");

		expect(seen[0]?.headers["X-CSRF-Token"]).toBe("csrf-1");
		expect(seen[0]?.headers["X-FloatCTF-Account"]).toBe("user-1");
		expect(seen[0]?.headers.Authorization).toBeUndefined();
		// 管理端不受选手 Cookie 会话影响
		expect(seen[1]?.headers.Authorization).toBe("Bearer admin-jwt");
		expect(seen[1]?.headers["X-CSRF-Token"]).toBeUndefined();
	});

	it("falls back to the bearer token when no session headers are available", async () => {
		const seen: Seen[] = [];
		const client = createFloatCTFClient({
			getUserToken: () => "legacy-jwt",
			getUserSessionHeaders: () => null,
		});
		capture(client.transport.service, seen);
		await client.serviceHttp.get("/users/me");
		expect(seen[0]?.headers.Authorization).toBe("Bearer legacy-jwt");
	});
});

describe("webSession api", () => {
	it("hits the web-session endpoints and passes an explicit csrf token before login", async () => {
		const seen: Seen[] = [];
		const client = createFloatCTFClient({ getUserSessionHeaders: () => null });
		capture(client.transport.service, seen);

		await client.service.webSession.status();
		await client.service.webSession.login({ username: "a", password: "b" }, "anonymous");
		await client.service.webSession.select("acc-1", "csrf");
		await client.service.webSession.logout("account", null, "csrf");
		await client.service.webSession.migrate("legacy-jwt");

		expect(seen.map((r) => `${r.method} ${r.url}`)).toEqual([
			"get /users/web-session",
			"post /users/web-session/login",
			"post /users/web-session/select",
			"post /users/web-session/logout",
			"post /users/web-session/migrate",
		]);
		expect(seen[1]?.headers["X-CSRF-Token"]).toBe("anonymous");
		expect(JSON.parse(String(seen[2]?.data))).toEqual({ account_id: "acc-1" });
		expect(JSON.parse(String(seen[3]?.data))).toEqual({ scope: "account", account_id: null });
		expect(seen[4]?.headers.Authorization).toBe("Bearer legacy-jwt");
		expect(seen[4]?.headers["X-CSRF-Token"]).toBe("anonymous");
	});

	it("rejects when logout fails so the UI cannot pretend the user signed out", async () => {
		const seen: Seen[] = [];
		const onUnauthorized = vi.fn();
		const client = createFloatCTFClient({ onUnauthorized });
		capture(client.transport.service, seen, () => ({
			status: 500,
			data: { code: 500, message: "服务暂时不可用" },
		}));
		await expect(client.service.webSession.logout("account")).rejects.toMatchObject({
			httpStatus: 500,
		});
		expect(onUnauthorized).not.toHaveBeenCalled();
	});
});

describe("cookie session SSE", () => {
	it("re-reads session headers on every connection and never adds a bearer", async () => {
		const calls: Record<string, string>[] = [];
		const fetchMock = vi.fn(async (_url: string, init: { headers: Record<string, string> }) => {
			calls.push(init.headers);
			return new Response("", { status: 401 });
		});
		vi.stubGlobal("fetch", fetchMock);
		let csrf = "first";
		const controller = new AbortController();
		const conn = connectSse({
			url: "/api/x",
			headers: {},
			signal: controller.signal,
			onEvent: () => undefined,
			getToken: () => "legacy-jwt",
			getHeaders: () => ({ "X-CSRF-Token": csrf }),
		});
		await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
		expect(calls[0]?.["X-CSRF-Token"]).toBe("first");
		expect(calls[0]?.Authorization).toBeUndefined();
		conn.close();
		csrf = "second";
		const again = connectSse({
			url: "/api/x",
			headers: {},
			signal: controller.signal,
			onEvent: () => undefined,
			getHeaders: () => ({ "X-CSRF-Token": csrf }),
		});
		await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
		expect(calls[1]?.["X-CSRF-Token"]).toBe("second");
		again.close();
		vi.unstubAllGlobals();
	});
});
