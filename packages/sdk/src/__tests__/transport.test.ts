import { describe, expect, it, vi } from "vitest";

import { createFloatCTFClient } from "../client";
import { FloatCTFError } from "../errors";
import {
	createFloatCTFTransport,
	normalizeBaseUrl,
	resolveBaseUrls,
} from "../transport";

/**
 * 传输层契约测试（取代旧 `apps/web/src/api/__tests__/axiosInterceptors.test.ts`）。
 *
 * 关键变化：
 * - SDK **不再**自己导航或清登录态，而是把 401 交给调用方注入的 `onUnauthorized`
 *   （"机制在 SDK、UI 反应在前端"这条边界）。
 * - SDK **不再**有模块级共享绑定：每个 transport 都是独立对象，因此这里不需要
 *   任何"测试用全局重置"。
 */

interface RecordedRequest {
	url?: string;
	baseURL?: string;
	headers: Record<string, unknown>;
}

interface AxiosLike {
	defaults: { adapter?: unknown };
}

function installAdapter(
	instance: AxiosLike,
	record: RecordedRequest[],
	respond: (config: { url?: string; baseURL?: string }) => {
		status: number;
		data?: unknown;
	},
): void {
	instance.defaults.adapter = async (config: Record<string, unknown>) => {
		const url = config.url as string | undefined;
		const baseURL = config.baseURL as string | undefined;
		record.push({
			url,
			baseURL,
			headers: (config.headers ?? {}) as Record<string, unknown>,
		});
		const { status, data } = respond({ url, baseURL });
		if (status >= 400) {
			const error = Object.assign(
				new Error(`Request failed with status code ${status}`),
				{
					isAxiosError: true,
					response: { status, data, statusText: "", headers: {} },
					config,
				},
			);
			throw error;
		}
		return { status, data, statusText: "", headers: {}, config };
	};
}

describe("transport base URL", () => {
	it("normalizes trailing slashes and derives the admin base from the player base", () => {
		expect(normalizeBaseUrl("https://a.example/api///")).toBe("https://a.example/api");
		expect(resolveBaseUrls({ baseUrl: "https://a.example/api/" })).toEqual({
			baseUrl: "https://a.example/api",
			adminBaseUrl: "https://a.example/api/admin",
		});
		expect(
			resolveBaseUrls({
				baseUrl: "https://a.example/api",
				adminBaseUrl: "https://admin.example/ctl/",
			}),
		).toEqual({
			baseUrl: "https://a.example/api",
			adminBaseUrl: "https://admin.example/ctl",
		});
	});

	it("treats the client baseUrl as authoritative even if requestConfig smuggles a baseURL", async () => {
		const seen: RecordedRequest[] = [];
		// 故意绕过类型（模拟 JS 调用方 / 旧代码）塞入 baseURL：
		// 客户端配置必须赢，否则 client.baseUrl 会与实际请求地址不一致。
		const client = createFloatCTFClient({
			baseUrl: "https://correct.example/api",
			requestConfig: { baseURL: "https://wrong.example/api" } as never,
		});
		installAdapter(client.transport.service, seen, () => ({ status: 200, data: {} }));

		await client.serviceHttp.get("/events");

		expect(client.baseUrl).toBe("https://correct.example/api");
		expect(seen[0]?.baseURL).toBe("https://correct.example/api");
		expect(seen[0]?.url).toBe("/events");
	});

	it("creates isolated transports without any module-level state", async () => {
		const seenA: RecordedRequest[] = [];
		const seenB: RecordedRequest[] = [];
		const a = createFloatCTFTransport("user", { getUserToken: () => "A" }, "/api-a");
		const b = createFloatCTFTransport("user", { getUserToken: () => "B" }, "/api-b");
		installAdapter(a.instance, seenA, () => ({ status: 200, data: {} }));
		installAdapter(b.instance, seenB, () => ({ status: 200, data: {} }));

		await b.http.get("/events");
		await a.http.get("/events");

		expect(seenA[0]?.baseURL).toBe("/api-a");
		expect(seenA[0]?.headers.Authorization).toBe("Bearer A");
		expect(seenB[0]?.baseURL).toBe("/api-b");
		expect(seenB[0]?.headers.Authorization).toBe("Bearer B");
	});
});

describe("transport interceptors", () => {
	it("sends the player bearer token on the player transport only", async () => {
		const seen: RecordedRequest[] = [];
		const client = createFloatCTFClient({
			baseUrl: "/api",
			getUserToken: () => "user-token",
			getAdminToken: () => "admin-token",
		});
		installAdapter(client.transport.service, seen, () => ({
			status: 200,
			data: { code: 0 },
		}));
		installAdapter(client.transport.admin, seen, () => ({ status: 200, data: { code: 0 } }));

		await client.serviceHttp.get("/events");
		await client.adminHttp.get("/settings");

		expect(seen[0]?.headers.Authorization).toBe("Bearer user-token");
		expect(seen[1]?.headers.Authorization).toBe("Bearer admin-token");
	});

	it("omits the Authorization header when there is no token", async () => {
		const seen: RecordedRequest[] = [];
		const client = createFloatCTFClient({ getUserToken: () => null });
		installAdapter(client.transport.service, seen, () => ({ status: 200, data: {} }));
		await client.serviceHttp.get("/events");
		expect(seen[0]?.headers.Authorization).toBeUndefined();
	});

	it("invokes onUnauthorized for a 401 instead of navigating anywhere", async () => {
		const onUnauthorized = vi.fn();
		const client = createFloatCTFClient({
			getUserToken: () => "user-token",
			getAdminToken: () => "admin-token",
			onUnauthorized,
		});
		installAdapter(client.transport.service, [], () => ({
			status: 401,
			data: { code: 401, message: "unauthorized" },
		}));

		await expect(client.serviceHttp.get("/events")).rejects.toBeInstanceOf(FloatCTFError);
		expect(onUnauthorized).toHaveBeenCalledTimes(1);
		expect(onUnauthorized.mock.calls[0][0]).toMatchObject({ scope: "user", status: 401 });
	});

	it("reports the admin scope separately", async () => {
		const onUnauthorized = vi.fn();
		const client = createFloatCTFClient({ onUnauthorized });
		installAdapter(client.transport.admin, [], () => ({ status: 401, data: {} }));

		await expect(client.adminHttp.get("/settings")).rejects.toBeInstanceOf(FloatCTFError);
		expect(onUnauthorized.mock.calls[0][0]).toMatchObject({ scope: "admin", status: 401 });
	});

	it("only notifies the client that owns the failing transport", async () => {
		const onUnauthorizedA = vi.fn();
		const onUnauthorizedB = vi.fn();
		const a = createFloatCTFClient({ onUnauthorized: onUnauthorizedA });
		const b = createFloatCTFClient({ onUnauthorized: onUnauthorizedB });
		installAdapter(a.transport.service, [], () => ({ status: 401, data: {} }));
		installAdapter(b.transport.service, [], () => ({ status: 401, data: {} }));

		await expect(a.serviceHttp.get("/events")).rejects.toBeInstanceOf(FloatCTFError);

		expect(onUnauthorizedA).toHaveBeenCalledTimes(1);
		expect(onUnauthorizedB).not.toHaveBeenCalled();
	});

	it("does not call onUnauthorized for 500 or for network failures", async () => {
		const onUnauthorized = vi.fn();
		const client = createFloatCTFClient({ onUnauthorized });
		installAdapter(client.transport.admin, [], () => ({
			status: 500,
			data: { message: "boom" },
		}));

		await expect(client.adminHttp.get("/settings")).rejects.toMatchObject({
			httpStatus: 500,
		});
		expect(onUnauthorized).not.toHaveBeenCalled();
	});

	it("classifies a response-less network error as kind=network without crashing", async () => {
		const onError = vi.fn();
		const client = createFloatCTFClient({ onError });
		client.transport.service.defaults.adapter = async () => {
			throw Object.assign(new Error("Network Error"), {
				isAxiosError: true,
				code: "ERR_NETWORK",
			});
		};

		const rejection = await client.serviceHttp.get("/events").catch((error) => error);
		expect(rejection).toBeInstanceOf(FloatCTFError);
		expect((rejection as FloatCTFError).kind).toBe("network");
		expect((rejection as FloatCTFError).httpStatus).toBeUndefined();
		expect(onError).toHaveBeenCalledTimes(1);
	});

	it("preserves platform code and message, and keeps axios-shaped response for existing UI", async () => {
		const client = createFloatCTFClient({});
		installAdapter(client.transport.service, [], () => ({
			status: 400,
			data: { code: 1001, message: "赛事已结束，无法提交 flag" },
		}));

		const rejection = (await client.serviceHttp
			.get("/submit/flag")
			.catch((error) => error)) as FloatCTFError;
		expect(rejection.httpStatus).toBe(400);
		expect(rejection.code).toBe(1001);
		expect(rejection.platformMessage).toBe("赛事已结束，无法提交 flag");
		// 既有页面读取 error.response?.data?.message —— 必须继续可用
		expect(
			(rejection.response?.data as { message: string } | undefined)?.message,
		).toBe("赛事已结束，无法提交 flag");
		expect(rejection.displayMessage).toBe("赛事已结束，无法提交 flag");
	});
});
