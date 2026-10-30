import { describe, expect, it } from "vitest";

import { createFloatCTFClient } from "../client";

/**
 * 角色 / 权限 API 的请求形状契约（Wave 8）。
 *
 * 这些用例锁定的是**路径与语义**，而不是后端行为：
 * 路径写错（例如把 `/roles/{id}` 写成 `/role/{id}`）在类型上完全看不出来，
 * 只会在运行时 404。RBAC 端点的路径错了会静默失去整块能力。
 */

interface Hit {
	method: string;
	url?: string;
	baseURL?: string;
	data?: unknown;
}

function install(
	instance: { defaults: { adapter?: unknown } },
	hits: Hit[],
	respondData: unknown = { code: 200, data: null },
): void {
	instance.defaults.adapter = async (config: Record<string, unknown>) => {
		hits.push({
			method: String(config.method ?? "").toLowerCase(),
			url: config.url as string | undefined,
			baseURL: config.baseURL as string | undefined,
			data: config.data,
		});
		return {
			status: 200,
			data: respondData,
			statusText: "OK",
			headers: {},
			config,
		};
	};
}

describe("admin.roles API 路径", () => {
	it("角色 CRUD 与权限目录都挂在 /admin 下且路径正确", async () => {
		const hits: Hit[] = [];
		const client = createFloatCTFClient({ adminBaseUrl: "/api/admin" });
		install(client.transport.admin, hits);

		await client.admin.roles.listPermissions();
		await client.admin.roles.listPermissionDomains();
		await client.admin.roles.fetch();
		await client.admin.roles.get("role-1");
		await client.admin.roles.create({
			key: "event-manager",
			name: "Event Manager",
			permissions: ["events.read"],
		});
		await client.admin.roles.patch("role-1", { name: "Renamed" });
		await client.admin.roles.remove("role-1");

		expect(hits.map((h) => `${h.method} ${h.url}`)).toEqual([
			"get /permissions",
			"get /permissions/domains",
			"get /roles",
			"get /roles/role-1",
			"post /roles",
			"patch /roles/role-1",
			"delete /roles/role-1",
		]);
		for (const hit of hits) {
			expect(hit.baseURL, "RBAC 端点必须在管理端 base URL 下").toBe("/api/admin");
		}
	});

	it("账号角色授予用 PUT 且发送 role_ids（整体替换语义）", async () => {
		const hits: Hit[] = [];
		const client = createFloatCTFClient({ adminBaseUrl: "/api/admin" });
		install(client.transport.admin, hits);

		await client.admin.users.getRoles("user-1");
		await client.admin.users.setRoles("user-1", ["r1", "r2"]);

		expect(hits[0]).toMatchObject({ method: "get", url: "/users/user-1/roles" });
		expect(hits[1]).toMatchObject({
			method: "put",
			url: "/users/user-1/roles",
		});
		expect(
			JSON.parse(String(hits[1]?.data)),
			"必须是整体替换的 role_ids 数组，而不是增量指令",
		).toEqual({ role_ids: ["r1", "r2"] });
	});

	it("角色授予不发送任何权限字段（防止前端自造授权）", async () => {
		const hits: Hit[] = [];
		const client = createFloatCTFClient({ adminBaseUrl: "/api/admin" });
		install(client.transport.admin, hits);

		await client.admin.users.setRoles("user-1", ["r1"]);

		const body = String(hits[0]?.data);
		expect(body).not.toContain("permissions");
		expect(body).not.toContain("grants_all");
		expect(
			body,
			"前端只能提交角色 id；权限必须由后端从角色推导（否则就是提权通道）",
		).not.toContain("is_admin");
	});
});
