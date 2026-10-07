import { afterEach, describe, expect, it, vi } from "vitest";

import { FloatCTFError } from "../errors";
import {
	bindHttpClients,
	resetHttpClientsForTests,
	service_api,
	admin_api,
} from "../transport";

/**
 * 传输层契约测试（取代旧 `apps/web/src/api/__tests__/axiosInterceptors.test.ts`）。
 *
 * 关键变化：SDK **不再**自己导航或清登录态，而是把 401 交给调用方注入的
 * `onUnauthorized`。这里验证"机制在 SDK、UI 反应在前端"这条边界。
 */

interface RecordedRequest {
	url?: string;
	headers: Record<string, unknown>;
}

function installAdapter(
	instance: { defaults: { adapter?: unknown } },
	record: RecordedRequest[],
	respond: (config: { url?: string }) => { status: number; data?: unknown },
): void {
	instance.defaults.adapter = async (config: Record<string, unknown>) => {
		record.push({
			url: config.url as string | undefined,
			headers: (config.headers ?? {}) as Record<string, unknown>,
		});
		const { status, data } = respond(config);
		if (status >= 400) {
			const error = Object.assign(new Error(`Request failed with status code ${status}`), {
				isAxiosError: true,
				response: { status, data, statusText: "", headers: {} },
				config,
			});
			throw error;
		}
		return { status, data, statusText: "", headers: {}, config };
	};
}

describe("createFloatCTFClient transport", () => {
	afterEach(() => {
		resetHttpClientsForTests();
		vi.restoreAllMocks();
	});

	it("sends the player bearer token on the player transport only", async () => {
		const seen: RecordedRequest[] = [];
		const bound = bindHttpClients({
			baseUrl: "/api",
			getUserToken: () => "user-token",
			getAdminToken: () => "admin-token",
		});
		installAdapter(bound.service, seen, () => ({ status: 200, data: { code: 0 } }));
		installAdapter(bound.admin, seen, () => ({ status: 200, data: { code: 0 } }));

		await service_api.get("/events");
		await admin_api.get("/settings");

		expect(seen[0]?.headers.Authorization).toBe("Bearer user-token");
		expect(seen[1]?.headers.Authorization).toBe("Bearer admin-token");
	});

	it("omits the Authorization header when there is no token", async () => {
		const seen: RecordedRequest[] = [];
		const bound = bindHttpClients({ getUserToken: () => null });
		installAdapter(bound.service, seen, () => ({ status: 200, data: {} }));
		await service_api.get("/events");
		expect(seen[0]?.headers.Authorization).toBeUndefined();
	});

	it("invokes onUnauthorized for a 401 instead of navigating anywhere", async () => {
		const onUnauthorized = vi.fn();
		const bound = bindHttpClients({
			getUserToken: () => "user-token",
			getAdminToken: () => "admin-token",
			onUnauthorized,
		});
		installAdapter(bound.service, [], () => ({
			status: 401,
			data: { code: 401, message: "unauthorized" },
		}));

		await expect(service_api.get("/events")).rejects.toBeInstanceOf(FloatCTFError);
		expect(onUnauthorized).toHaveBeenCalledTimes(1);
		expect(onUnauthorized.mock.calls[0][0]).toMatchObject({ scope: "user", status: 401 });
	});

	it("reports the admin scope separately", async () => {
		const onUnauthorized = vi.fn();
		const bound = bindHttpClients({ onUnauthorized });
		installAdapter(bound.admin, [], () => ({ status: 401, data: {} }));

		await expect(admin_api.get("/settings")).rejects.toBeInstanceOf(FloatCTFError);
		expect(onUnauthorized.mock.calls[0][0]).toMatchObject({ scope: "admin", status: 401 });
	});

	it("does not call onUnauthorized for 500 or for network failures", async () => {
		const onUnauthorized = vi.fn();
		const bound = bindHttpClients({ onUnauthorized });
		installAdapter(bound.admin, [], () => ({ status: 500, data: { message: "boom" } }));

		await expect(admin_api.get("/settings")).rejects.toMatchObject({ httpStatus: 500 });
		expect(onUnauthorized).not.toHaveBeenCalled();
	});

	it("classifies a response-less network error as kind=network without crashing", async () => {
		const onError = vi.fn();
		const bound = bindHttpClients({ onError });
		bound.service.defaults.adapter = async () => {
			throw Object.assign(new Error("Network Error"), {
				isAxiosError: true,
				code: "ERR_NETWORK",
			});
		};

		const rejection = await service_api.get("/events").catch((error) => error);
		expect(rejection).toBeInstanceOf(FloatCTFError);
		expect((rejection as FloatCTFError).kind).toBe("network");
		expect((rejection as FloatCTFError).httpStatus).toBeUndefined();
		expect(onError).toHaveBeenCalledTimes(1);
	});

	it("preserves platform code and message, and keeps axios-shaped response for existing UI", async () => {
		const bound = bindHttpClients({});
		installAdapter(bound.service, [], () => ({
			status: 400,
			data: { code: 1001, message: "赛事已结束，无法提交 flag" },
		}));

		const rejection = (await service_api
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

	it("throws a readable error when used before binding", async () => {
		resetHttpClientsForTests();
		await expect(service_api.get("/events")).rejects.toThrow(/not configured/);
	});
});
