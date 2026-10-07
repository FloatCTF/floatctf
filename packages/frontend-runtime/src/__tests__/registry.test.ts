import { describe, expect, it } from "vitest";

import {
	type FloatCTFRegistry,
	emptyRegistry,
	parseRegistry,
	registerFrontendVersion,
	resolveFrontend,
} from "../registry";

function versionEntry(overrides: Record<string, unknown> = {}) {
	return {
		version: "1.0.0",
		name: "Default Frontend",
		compatibility: { frontendRuntime: "1", apiContract: "1" },
		entry: "assets/frontend.js",
		styles: ["assets/frontend.css"],
		installedAt: "2026-10-07T00:00:00.000Z",
		...overrides,
	};
}

/**
 * 刻意构造"非法注册表"用的可变视图。
 *
 * 测试必须能塞进越界字段（坏 currentVersion / 穿越路径 / 不安全 ID），
 * 因此这里给一个**显式**的可变形状，而不是 `any`（避免关闭类型检查）。
 */
type MutableRegistry = {
	frontends: Record<
		string,
		{
			id?: string;
			currentVersion?: string;
			protected?: boolean;
			versions: Record<string, { entry?: unknown; version?: unknown }>;
		}
	>;
};

function mutableRegistry(): MutableRegistry {
	return validRegistry() as MutableRegistry;
}

function validRegistry(): unknown {
	return {
		schemaVersion: 1,
		updatedAt: "2026-10-07T00:00:00.000Z",
		frontends: {
			default: {
				id: "default",
				currentVersion: "1.0.0",
				protected: true,
				versions: { "1.0.0": versionEntry() },
			},
			cyberpunk: {
				id: "cyberpunk",
				currentVersion: "1.2.0",
				versions: {
					"1.1.0": versionEntry({ version: "1.1.0", name: "Cyberpunk" }),
					"1.2.0": versionEntry({ version: "1.2.0", name: "Cyberpunk" }),
				},
			},
		},
	};
}

describe("parseRegistry", () => {
	it("accepts a valid registry and keeps explicit current versions", () => {
		const result = parseRegistry(validRegistry());
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		expect(Object.keys(result.registry.frontends).sort()).toEqual([
			"cyberpunk",
			"default",
		]);
		expect(result.registry.frontends.cyberpunk.currentVersion).toBe("1.2.0");
	});

	it("rejects an unsupported registry schema version", () => {
		const raw = validRegistry() as Record<string, unknown>;
		raw.schemaVersion = 2;
		const result = parseRegistry(raw);
		expect(result.ok).toBe(false);
		if (result.ok) return;
		expect(result.errors.join("\n")).toContain("unsupported schemaVersion");
	});

	it("rejects a currentVersion that is not installed", () => {
		const raw = mutableRegistry();
		raw.frontends.cyberpunk.currentVersion = "9.9.9";
		const result = parseRegistry(raw);
		expect(result.ok).toBe(false);
		if (result.ok) return;
		expect(result.errors.join("\n")).toContain("currentVersion");
	});

	it("rejects traversal paths inside the registry", () => {
		const raw = mutableRegistry();
		raw.frontends.cyberpunk.versions["1.2.0"].entry = "../../../evil.js";
		const result = parseRegistry(raw);
		expect(result.ok).toBe(false);
	});

	it("rejects unsafe frontend ids", () => {
		const raw = mutableRegistry();
		raw.frontends["../evil"] = raw.frontends.cyberpunk;
		const result = parseRegistry(raw);
		expect(result.ok).toBe(false);
		if (result.ok) return;
		expect(result.errors.join("\n")).toContain("not a safe id");
	});

	it("rejects an empty frontend set", () => {
		const raw = validRegistry() as Record<string, unknown>;
		raw.frontends = {};
		const result = parseRegistry(raw);
		expect(result.ok).toBe(false);
	});

	it("rejects malformed JSON", () => {
		expect(parseRegistry("{").ok).toBe(false);
	});
});

describe("registerFrontendVersion", () => {
	const now = "2026-10-07T00:00:00.000Z";

	it("rejects duplicate frontend/version registration", () => {
		const base = emptyRegistry(now);
		const first = registerFrontendVersion(base, {
			id: "cyberpunk",
			version: { ...versionEntry(), version: "1.0.0" },
		});
		expect(first.ok).toBe(true);
		if (!first.ok) return;
		const second = registerFrontendVersion(first.registry, {
			id: "cyberpunk",
			version: { ...versionEntry(), version: "1.0.0" },
		});
		expect(second.ok).toBe(false);
		if (second.ok) return;
		expect(second.error).toContain("already installed");
	});

	it("keeps the existing currentVersion unless makeCurrent is requested", () => {
		const base = emptyRegistry(now);
		const first = registerFrontendVersion(base, {
			id: "cyberpunk",
			version: { ...versionEntry(), version: "1.0.0" },
		});
		if (!first.ok) throw new Error(first.error);
		const second = registerFrontendVersion(first.registry, {
			id: "cyberpunk",
			version: { ...versionEntry(), version: "1.1.0" },
		});
		if (!second.ok) throw new Error(second.error);
		expect(second.registry.frontends.cyberpunk.currentVersion).toBe("1.0.0");

		const third = registerFrontendVersion(
			second.registry,
			{ id: "cyberpunk", version: { ...versionEntry(), version: "1.1.0" } },
			false,
		);
		expect(third.ok).toBe(false);
	});

	it("moves the pointer when makeCurrent is requested", () => {
		const base = emptyRegistry(now);
		const first = registerFrontendVersion(base, {
			id: "cyberpunk",
			version: { ...versionEntry(), version: "1.0.0" },
		});
		if (!first.ok) throw new Error(first.error);
		const second = registerFrontendVersion(
			first.registry,
			{ id: "cyberpunk", version: { ...versionEntry(), version: "1.1.0" } },
			true,
		);
		if (!second.ok) throw new Error(second.error);
		expect(second.registry.frontends.cyberpunk.currentVersion).toBe("1.1.0");
	});

	it("rejects unsafe ids and invalid versions", () => {
		const base = emptyRegistry(now);
		expect(
			registerFrontendVersion(base, { id: "../x", version: versionEntry() }).ok,
		).toBe(false);
		expect(
			registerFrontendVersion(base, {
				id: "ok",
				version: { ...versionEntry(), version: "1.0" },
			}).ok,
		).toBe(false);
	});
});

describe("resolveFrontend", () => {
	function parsed(): FloatCTFRegistry {
		const result = parseRegistry(validRegistry());
		if (!result.ok) throw new Error(result.errors.join("; "));
		return result.registry;
	}

	it("resolves same-origin asset URLs from validated relative paths", () => {
		const result = resolveFrontend(parsed(), {
			requestedId: "cyberpunk",
			frontendBaseUrl: "/__floatctf/frontends",
		});
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		expect(result.frontend.version).toBe("1.2.0");
		expect(result.frontend.entryUrl).toBe(
			"/__floatctf/frontends/cyberpunk/1.2.0/assets/frontend.js",
		);
		expect(result.frontend.styleUrls).toEqual([
			"/__floatctf/frontends/cyberpunk/1.2.0/assets/frontend.css",
		]);
	});

	it("fails when the requested frontend is not installed (caller falls back to default)", () => {
		const result = resolveFrontend(parsed(), {
			requestedId: "does-not-exist",
			frontendBaseUrl: "/__floatctf/frontends",
		});
		expect(result.ok).toBe(false);
		if (result.ok) return;
		expect(result.errors.join("\n")).toContain("is not installed");
	});

	it("fails on an unsafe requested id without touching the filesystem layout", () => {
		const result = resolveFrontend(parsed(), {
			requestedId: "../../etc/passwd",
			frontendBaseUrl: "/__floatctf/frontends",
		});
		expect(result.ok).toBe(false);
	});

	it("fails on incompatible runtime/API contract majors", () => {
		const registry = parsed();
		registry.frontends.cyberpunk.versions["1.2.0"].compatibility = {
			frontendRuntime: "2",
			apiContract: "1",
		};
		const result = resolveFrontend(registry, {
			requestedId: "cyberpunk",
			frontendBaseUrl: "/__floatctf/frontends",
		});
		expect(result.ok).toBe(false);
		if (result.ok) return;
		expect(result.errors.join("\n")).toContain("requires frontend runtime 2");

		const registry2 = parsed();
		registry2.frontends.cyberpunk.versions["1.2.0"].compatibility = {
			frontendRuntime: "1",
			apiContract: "7",
		};
		const result2 = resolveFrontend(registry2, {
			requestedId: "cyberpunk",
			frontendBaseUrl: "/__floatctf/frontends",
		});
		expect(result2.ok).toBe(false);
	});

	it("never picks by lexicographic ordering (9.0.0 is not chosen over 10.0.0)", () => {
		const registry = parseRegistry({
			schemaVersion: 1,
			updatedAt: "2026-10-07T00:00:00.000Z",
			frontends: {
				demo: {
					id: "demo",
					currentVersion: "9.0.0",
					versions: {
						"9.0.0": versionEntry({ version: "9.0.0" }),
						"10.0.0": versionEntry({ version: "10.0.0" }),
					},
				},
			},
		});
		expect(registry.ok).toBe(true);
		if (!registry.ok) return;
		const result = resolveFrontend(registry.registry, {
			requestedId: "demo",
			frontendBaseUrl: "/__floatctf/frontends",
		});
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		expect(result.frontend.version).toBe("9.0.0");
	});
});

describe("public registry schema is fail-closed", () => {
	const reject = (mutate: (registry: Record<string, unknown>) => void) => {
		const raw = validRegistry() as Record<string, unknown>;
		mutate(raw);
		const result = parseRegistry(raw);
		expect(result.ok).toBe(false);
		if (result.ok) return [];
		return result.errors;
	};

	it("rejects unknown root fields", () => {
		const errors = reject((registry) => {
			registry.operator = "alice";
		});
		expect(errors.join("\n")).toContain("unknown field `operator`");
	});

	it("rejects unknown frontend entry fields", () => {
		const errors = reject((registry) => {
			const frontends = registry.frontends as Record<string, Record<string, unknown>>;
			frontends.default.installedBy = "root";
		});
		expect(errors.join("\n")).toContain("unknown field `installedBy`");
	});

	it("rejects unknown version entry fields (including legacy `source`)", () => {
		const errors = reject((registry) => {
			const frontends = registry.frontends as Record<
				string,
				{ versions: Record<string, Record<string, unknown>> }
			>;
			frontends.default.versions["1.0.0"].source = "path:/home/alice/private";
		});
		expect(errors.join("\n")).toContain("unknown field `source`");
	});

	it("rejects credential-looking metadata anywhere in a version entry", () => {
		for (const key of ["token", "localPath", "cloneUrl", "clone_url", "credentials"]) {
			const errors = reject((registry) => {
				const frontends = registry.frontends as Record<
					string,
					{ versions: Record<string, Record<string, unknown>> }
				>;
				frontends.default.versions["1.0.0"][key] = "https://user:token@example/repo.git";
			});
			expect(errors.join("\n")).toContain(`unknown field \`${key}\``);
		}
	});

	it("rejects unknown compatibility fields", () => {
		const errors = reject((registry) => {
			const frontends = registry.frontends as Record<
				string,
				{ versions: Record<string, { compatibility: Record<string, unknown> }> }
			>;
			frontends.default.versions["1.0.0"].compatibility.buildHost = "ci-1";
		});
		expect(errors.join("\n")).toContain("unknown field `buildHost`");
	});

	it("still accepts a registry written by scripts/frontend.sh", () => {
		// 与 CLI 的 sanitize_registry 输出保持一致：只有公开字段。
		const result = parseRegistry(validRegistry());
		expect(result.ok).toBe(true);
	});
});
