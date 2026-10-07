import { describe, expect, it } from "vitest";

import { createFloatCTFClient, resolveSseUrl } from "../client";

/**
 * 多客户端隔离测试（Phase 12.1 / P0-A）。
 *
 * 这些用例存在的理由：SDK 曾经用**模块级绑定**（`service_api` / `admin_api` +
 * `bindHttpClients()`）实现传输，于是 `createFloatCTFClient()` 创建的多个实例
 * 会互相覆盖 —— 后创建的客户端会把先前客户端的请求"劫持"到自己的 base URL/token。
 * 作为公开 SDK 这是不可接受的，因此现在每个客户端完全独立。
 *
 * 这里不使用任何"测试用全局重置"：如果还需要那种 helper，说明隔离又坏了。
 */

interface Recorded {
	baseURL?: string;
	url?: string;
	authorization?: string;
}

function record(instance: {
	defaults: { adapter?: unknown };
}, sink: Recorded[], tag: { status?: number; data?: unknown } = {}): void {
	instance.defaults.adapter = async (config: Record<string, unknown>) => {
		const headers = (config.headers ?? {}) as Record<string, unknown>;
		sink.push({
			baseURL: config.baseURL as string | undefined,
			url: config.url as string | undefined,
			authorization: headers.Authorization as string | undefined,
		});
		const status = tag.status ?? 200;
		if (status >= 400) {
			throw Object.assign(new Error(`Request failed with status code ${status}`), {
				isAxiosError: true,
				response: { status, data: tag.data ?? {}, statusText: "", headers: {} },
				config,
			});
		}
		return {
			status,
			data: tag.data ?? { code: 0, message: "OK", data: null },
			statusText: "",
			headers: {},
			config,
		};
	};
}

const A_BASE = "https://a.example/api";
const B_BASE = "https://b.example/api";

describe("SDK client instance isolation", () => {
	it("keeps A on A's transport after B is created (and vice versa)", async () => {
		const hitsA: Recorded[] = [];
		const hitsB: Recorded[] = [];
		const a = createFloatCTFClient({ baseUrl: A_BASE, getUserToken: () => "TOKEN_A" });
		const b = createFloatCTFClient({ baseUrl: B_BASE, getUserToken: () => "TOKEN_B" });
		record(a.transport.service, hitsA);
		record(b.transport.service, hitsB);

		// B 先发，再发 A（创建顺序与调用顺序都试一遍）
		await b.service.events.fetch();
		await a.service.events.fetch();

		expect(hitsA).toHaveLength(1);
		expect(hitsB).toHaveLength(1);
		expect(hitsA[0]).toMatchObject({
			baseURL: A_BASE,
			url: "/events",
			authorization: "Bearer TOKEN_A",
		});
		expect(hitsB[0]).toMatchObject({
			baseURL: B_BASE,
			url: "/events",
			authorization: "Bearer TOKEN_B",
		});
	});

	it("keeps isolation with the reversed creation order", async () => {
		const hitsB: Recorded[] = [];
		const hitsA: Recorded[] = [];
		const b = createFloatCTFClient({ baseUrl: B_BASE, getUserToken: () => "TOKEN_B" });
		const a = createFloatCTFClient({ baseUrl: A_BASE, getUserToken: () => "TOKEN_A" });
		record(b.transport.service, hitsB);
		record(a.transport.service, hitsA);

		await a.service.events.fetch();
		await b.service.events.fetch();

		expect(hitsA[0]?.baseURL).toBe(A_BASE);
		expect(hitsA[0]?.authorization).toBe("Bearer TOKEN_A");
		expect(hitsB[0]?.baseURL).toBe(B_BASE);
		expect(hitsB[0]?.authorization).toBe("Bearer TOKEN_B");
	});

	it("keeps isolation under parallel requests", async () => {
		const hitsA: Recorded[] = [];
		const hitsB: Recorded[] = [];
		const a = createFloatCTFClient({ baseUrl: A_BASE, getUserToken: () => "TOKEN_A" });
		const b = createFloatCTFClient({ baseUrl: B_BASE, getUserToken: () => "TOKEN_B" });
		record(a.transport.service, hitsA);
		record(b.transport.service, hitsB);

		await Promise.all([
			a.service.events.fetch(),
			b.service.events.fetch(),
			a.service.events.fetch(),
			b.service.events.fetch(),
		]);

		expect(hitsA.map((h) => h.baseURL)).toEqual([A_BASE, A_BASE]);
		expect(hitsB.map((h) => h.baseURL)).toEqual([B_BASE, B_BASE]);
		expect(new Set(hitsA.map((h) => h.authorization))).toEqual(new Set(["Bearer TOKEN_A"]));
		expect(new Set(hitsB.map((h) => h.authorization))).toEqual(new Set(["Bearer TOKEN_B"]));
	});

	it("keeps player and admin transports isolated inside a single client", async () => {
		const hitsPlayer: Recorded[] = [];
		const hitsAdmin: Recorded[] = [];
		const client = createFloatCTFClient({
			baseUrl: A_BASE,
			getUserToken: () => "TOKEN_USER",
			getAdminToken: () => "TOKEN_ADMIN",
		});
		record(client.transport.service, hitsPlayer);
		record(client.transport.admin, hitsAdmin);

		await client.service.events.fetch();
		await client.admin.settings.fetch();

		expect(hitsPlayer[0]).toMatchObject({
			baseURL: A_BASE,
			authorization: "Bearer TOKEN_USER",
		});
		expect(hitsAdmin[0]).toMatchObject({
			baseURL: `${A_BASE}/admin`,
			authorization: "Bearer TOKEN_ADMIN",
		});
	});

	it("does not leak a second client's token when the first has none", async () => {
		const hitsA: Recorded[] = [];
		const hitsB: Recorded[] = [];
		const a = createFloatCTFClient({ baseUrl: A_BASE, getUserToken: () => null });
		const b = createFloatCTFClient({ baseUrl: B_BASE, getUserToken: () => "TOKEN_B" });
		record(a.transport.service, hitsA);
		record(b.transport.service, hitsB);

		await b.service.events.fetch();
		await a.service.events.fetch();

		expect(hitsA[0]?.authorization).toBeUndefined();
		expect(hitsA[0]?.baseURL).toBe(A_BASE);
		expect(hitsB[0]?.authorization).toBe("Bearer TOKEN_B");
	});

	it("routes AWD/AWDP facades through their own client", async () => {
		const hitsA: Recorded[] = [];
		const hitsB: Recorded[] = [];
		const a = createFloatCTFClient({ baseUrl: A_BASE, getUserToken: () => "TOKEN_A" });
		const b = createFloatCTFClient({ baseUrl: B_BASE, getUserToken: () => "TOKEN_B" });
		record(a.transport.service, hitsA);
		record(b.transport.service, hitsB);

		const eventId = "11111111-1111-1111-1111-111111111111";
		await b.awd.player.status(eventId);
		await a.awd.player.status(eventId);

		expect(hitsA[0]?.baseURL).toBe(A_BASE);
		expect(hitsA[0]?.url).toBe(`/events/${eventId}/awd/status`);
		expect(hitsB[0]?.baseURL).toBe(B_BASE);
	});

	it("resolves SSE URLs against the owning client base URL", () => {
		const a = createFloatCTFClient({ baseUrl: A_BASE });
		const b = createFloatCTFClient({ baseUrl: "http://127.0.0.1:17780/api" });

		// 相对路径 → 该客户端自己的 base URL；绝对 URL 原样保留。
		expect(resolveSseUrl(a.baseUrl, "/events/x/awd/stream")).toBe(
			`${A_BASE}/events/x/awd/stream`,
		);
		expect(resolveSseUrl(b.baseUrl, "/events/x/awd/stream")).toBe(
			"http://127.0.0.1:17780/api/events/x/awd/stream",
		);
		expect(resolveSseUrl(a.adminBaseUrl, "/events/x/awd/stream")).toBe(
			`${A_BASE}/admin/events/x/awd/stream`,
		);
		expect(resolveSseUrl(a.baseUrl, "https://other.example/stream")).toBe(
			"https://other.example/stream",
		);
		expect(a.adminBaseUrl).toBe(`${A_BASE}/admin`);
		expect(b.adminBaseUrl).toBe("http://127.0.0.1:17780/api/admin");
	});

	it("has no module-global binding helpers to reset", async () => {
		const sdk = await import("../index.js");
		for (const legacy of [
			"service_api",
			"admin_api",
			"bindHttpClients",
			"resetHttpClientsForTests",
			"httpClients",
		]) {
			expect(legacy in sdk).toBe(false);
		}
		// 正常的 SDK 行为不需要任何测试用全局重置：一个客户端发完请求，
		// 另一个仍然指向自己的地址。
		const hitsA: Recorded[] = [];
		const hitsB: Recorded[] = [];
		const a = createFloatCTFClient({ baseUrl: A_BASE, getUserToken: () => "TOKEN_A" });
		record(a.transport.service, hitsA);
		const b = createFloatCTFClient({ baseUrl: B_BASE, getUserToken: () => "TOKEN_B" });
		record(b.transport.service, hitsB);
		await b.service.events.fetch();
		await a.service.events.fetch();
		expect(hitsA[0]?.baseURL).toBe(A_BASE);
		expect(hitsB[0]?.baseURL).toBe(B_BASE);
	});
});
