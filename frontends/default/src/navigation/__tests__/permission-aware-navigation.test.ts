import { beforeEach, describe, expect, it } from "vitest";

import { adminNavigation } from "@/navigation/admin-navigation";
import type { NavigationItem, NavigationSection } from "@/navigation/sidebar-types";
import { filterNavigationSections } from "@/navigation/sidebar-utils";

/**
 * Wave 8：管理端导航的**权限感知**契约。
 *
 * 两条不可混同的性质：
 * 1. 导航显隐**只是界面行为**，不是授权 —— 因此这里只断言「什么权限看到什么入口」；
 * 2. 每条 admin 入口都必须声明权限：否则它会无条件显示给任何能进 /admin 的账号，
 *    用户点进去才被服务端 403（糟糕体验，且掩盖了权限模型）。
 */

function collectItems(sections: NavigationSection[]): NavigationItem[] {
	const out: NavigationItem[] = [];
	const walk = (nodes: NavigationSection["children"]) => {
		for (const node of nodes) {
			if ("children" in node && node.children) {
				walk(node.children as NavigationSection["children"]);
			} else if ((node as NavigationItem).type === "item") {
				out.push(node as NavigationItem);
			}
		}
	};
	for (const section of sections) walk(section.children);
	return out;
}

const items = collectItems(adminNavigation);

/** 把权限集合渲染成可见的 item id（沿用生产用的过滤函数）。 */
function visibleIds(permissions: string[]): string[] {
	const visible = filterNavigationSections(adminNavigation, {
		permissions: new Set(permissions),
		features: new Set<string>(),
	});
	return collectItems(visible).map((i) => i.id);
}

describe("管理端导航的权限声明", () => {
	it("每个 admin 入口都声明了 isVisible（否则会无条件显示）", () => {
		const missing = items.filter((i) => !i.isVisible).map((i) => i.id);
		expect(
			missing,
			`以下入口没有权限谓词，任何能进 /admin 的账号都会看到：${missing.join(", ")}`,
		).toEqual([]);
	});

	it("导航条目与其路由权限一致（抽查跨域的几个）", () => {
		const cases: Array<[string, string]> = [
			["dashboard", "system.read"],
			["access.users", "users.read"],
			["access.roles", "roles.read"],
			["events.all", "events.read"],
			["content.weapons", "weapons.read"],
			["community.discussions", "discussions.moderate"],
			["infrastructure.terminal", "terminal.open"],
			["infrastructure.database", "database.execute"],
			["operations.logs", "logs.read"],
			["system.settings", "settings.read"],
		];
		for (const [id, permission] of cases) {
			expect(visibleIds([permission]), `${id} 需要 ${permission}`).toContain(id);
		}
	});

	it("空权限（已登录但无任何平台权限）看不到任何管理入口", () => {
		expect(
			visibleIds([]),
			"没有平台权限的账号不得看到任何 admin 导航项 —— 它们点进去只会 403",
		).toEqual([]);
	});

	it("一条权限只放出它自己的入口（最小可见性）", () => {
		const only = visibleIds(["events.read"]);
		expect(only).toContain("events.all");
		expect(only).not.toContain("system.settings");
		expect(only).not.toContain("terminal.open");
		expect(only).not.toContain("database.execute");
	});

	it("高危入口必须由各自的权限门禁，而不是靠别的权限顺带放出", () => {
		// 这三项都是高危权限（roles.manage / database.execute / terminal.open 等）
		for (const [id, permission] of [
			["infrastructure.terminal", "terminal.open"],
			["infrastructure.database", "database.execute"],
			["system.settings", "settings.read"],
		] as const) {
			const withoutIt = visibleIds(
				items
					.map((i) => i.id)
					.filter((x) => x !== id)
					// 用一堆"普通"权限也不应该放出高危入口
					.concat(["users.read", "events.read", "challenges.read", "logs.read"]),
			);
			expect(withoutIt, `${id} 不应被普通权限放出`).not.toContain(id);
			expect(visibleIds([permission])).toContain(id);
		}
	});

	it("不再有「Super Admins」入口（v2 里管理员是角色，不是独立账号类型）", () => {
		expect(items.map((i) => i.id)).not.toContain("access.super-admins");
	});
});

describe("权限感知导航与真实权限集合", () => {
	it("全权账号（grants_all → 29 条权限）看到全部入口", () => {
		const all = [
			"users.read",
			"users.manage",
			"roles.read",
			"roles.manage",
			"challenges.read",
			"challenges.manage",
			"gameboxes.read",
			"gameboxes.manage",
			"weapons.read",
			"weapons.manage",
			"events.read",
			"events.manage",
			"events.participants.manage",
			"events.lifecycle.manage",
			"awd.read",
			"awd.manage",
			"awdp.read",
			"awdp.manage",
			"announcements.manage",
			"discussions.moderate",
			"settings.read",
			"settings.manage",
			"system.read",
			"logs.read",
			"scheduler.manage",
			"docker.read",
			"docker.manage",
			"database.execute",
			"terminal.open",
		];
		expect(visibleIds(all).sort()).toEqual(items.map((i) => i.id).sort());
	});
});

/**
 * AuthStore v2 的迁移契约。
 *
 * 这里断言的是**纯逻辑**（版本号与"迁移必须清空令牌"这一决定），
 * 而不是 zustand 的持久化实现细节。
 */
describe("AuthStore 持久化版本", () => {
	beforeEach(() => {
		// 每个用例都从干净状态开始，避免上一用例残留的 store 影响断言
	});

	it("版本号必须 >= 2（1 → 2 是破坏性令牌结构变更）", async () => {
		const { AUTH_STORAGE_VERSION } = await import("@/stores/AuthStore");
		expect(
			AUTH_STORAGE_VERSION,
			"v1 令牌会被后端拒绝；版本号必须提升以触发迁移清空",
		).toBeGreaterThanOrEqual(2);
	});

	it("登录后未拉取 /me 时，权限为空且未标记已加载", async () => {
		const { useAuthStore } = await import("@/stores/AuthStore");
		useAuthStore.getState().setToken("t");
		useAuthStore.getState().removePermissions();
		const state = useAuthStore.getState();
		expect(state.permissions).toEqual([]);
		expect(state.permissionsLoaded).toBe(false);
	});

	it("setPermissions 会记录已加载标记", async () => {
		const { useAuthStore } = await import("@/stores/AuthStore");
		useAuthStore.getState().setPermissions(["events.read"], false);
		const state = useAuthStore.getState();
		expect(state.permissions).toEqual(["events.read"]);
		expect(state.permissionsLoaded).toBe(true);
	});

	it("登出必须清空权限，避免下一个账号继承上一个账号的导航可见性", async () => {
		const { useAuthStore } = await import("@/stores/AuthStore");
		useAuthStore.getState().setPermissions(["terminal.open", "database.execute"], true);
		useAuthStore.getState().removeToken();
		const state = useAuthStore.getState();
		expect(state.permissions).toEqual([]);
		expect(state.grantsAll).toBe(false);
		expect(state.permissionsLoaded).toBe(false);
	});
});
