import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { bootstrapFrontend } from "../bootstrap";
import type { FloatCTFFrontendModule, FloatCTFMountContext } from "../module";

const REGISTRY = {
	schemaVersion: 1,
	updatedAt: "2026-10-07T00:00:00.000Z",
	frontends: {
		default: {
			id: "default",
			currentVersion: "1.0.0",
			protected: true,
			versions: {
				"1.0.0": {
					version: "1.0.0",
					name: "Default Frontend",
					compatibility: { frontendRuntime: "1", apiContract: "1" },
					entry: "assets/frontend.js",
					styles: ["assets/frontend.css"],
					installedAt: "2026-10-07T00:00:00.000Z",
				},
			},
		},
		cyberpunk: {
			id: "cyberpunk",
			currentVersion: "1.0.0",
			versions: {
				"1.0.0": {
					version: "1.0.0",
					name: "Cyberpunk",
					compatibility: { frontendRuntime: "1", apiContract: "1" },
					entry: "assets/frontend.js",
					styles: ["assets/frontend.css"],
					installedAt: "2026-10-07T00:00:00.000Z",
				},
			},
		},
		"legacy-runtime": {
			id: "legacy-runtime",
			currentVersion: "1.0.0",
			versions: {
				"1.0.0": {
					version: "1.0.0",
					name: "Legacy",
					compatibility: { frontendRuntime: "9", apiContract: "9" },
					entry: "assets/frontend.js",
					styles: [],
					installedAt: "2026-10-07T00:00:00.000Z",
				},
			},
		},
	},
};

function makeFetch(options: {
	activeFrontend?: string;
	bootstrapStatus?: number;
	registryStatus?: number;
}): typeof fetch {
	const activeFrontend = options.activeFrontend ?? "default";
	return (async (input: RequestInfo | URL) => {
		const url = String(input);
		if (url.endsWith("/frontend")) {
			if (options.bootstrapStatus && options.bootstrapStatus !== 200) {
				return new Response("nope", { status: options.bootstrapStatus });
			}
			return new Response(
				JSON.stringify({
					code: 0,
					message: "ok",
					data: {
						active_frontend: activeFrontend,
						platform_version: "1.0.0",
						api_contract_version: "1",
						frontend_runtime_version: "1",
						capabilities: ["awd", "awdp", "jeopardy"],
					},
				}),
				{ status: 200, headers: { "Content-Type": "application/json" } },
			);
		}
		if (options.registryStatus && options.registryStatus !== 200) {
			return new Response("nope", { status: options.registryStatus });
		}
		return new Response(JSON.stringify(REGISTRY), {
			status: 200,
			headers: { "Content-Type": "application/json" },
		});
	}) as typeof fetch;
}

function mountableModule(label: string): FloatCTFFrontendModule {
	return {
		mount: (context) => {
			context.root.textContent = label;
		},
	};
}

describe("bootstrapFrontend", () => {
	let root: HTMLElement;
	let emergency: HTMLElement;

	beforeEach(() => {
		document.body.innerHTML = '<div id="app"></div><div id="emergency"></div>';
		root = document.getElementById("app") as HTMLElement;
		emergency = document.getElementById("emergency") as HTMLElement;
	});

	afterEach(() => {
		vi.restoreAllMocks();
		document.head.innerHTML = "";
	});

	it("loads the active frontend and mounts it with a safe context", async () => {
		const contexts: unknown[] = [];
		const result = await bootstrapFrontend({
			root,
			emergencyHost: emergency,
			fetchImpl: makeFetch({ activeFrontend: "cyberpunk" }),
			importModule: async () => ({
				mount: (context: FloatCTFMountContext) => {
					contexts.push(context);
					context.root.textContent = "cyberpunk";
				},
			}),
			loadStyle: () => document.createElement("link"),
		});

		expect(result.mounted).toBe(true);
		expect(result.frontendId).toBe("cyberpunk");
		expect(root.textContent).toBe("cyberpunk");
		expect(contexts).toHaveLength(1);
		const context = contexts[0] as Record<string, unknown>;
		expect(context.apiBaseUrl).toBe("/api");
		expect(context.assetBaseUrl).toBe("/__floatctf/frontends/cyberpunk/1.0.0");
		expect(context.capabilities).toEqual(["awd", "awdp", "jeopardy"]);
		// 契约上不得出现路由/页面概念
		expect(context).not.toHaveProperty("router");
		expect(context).not.toHaveProperty("queryClient");
	});

	it("imports only registry-derived same-origin URLs", async () => {
		const imported: string[] = [];
		await bootstrapFrontend({
			root,
			emergencyHost: emergency,
			fetchImpl: makeFetch({ activeFrontend: "cyberpunk" }),
			importModule: async (url) => {
				imported.push(url);
				return mountableModule("x");
			},
			loadStyle: () => document.createElement("link"),
		});
		expect(imported).toEqual(["/__floatctf/frontends/cyberpunk/1.0.0/assets/frontend.js"]);
	});

	it("falls back to default when active frontend is not installed", async () => {
		const result = await bootstrapFrontend({
			root,
			emergencyHost: emergency,
			fetchImpl: makeFetch({ activeFrontend: "not-installed" }),
			importModule: async (url) => {
				if (url.includes("/default/")) return mountableModule("default");
				throw new Error("should not be imported");
			},
			loadStyle: () => document.createElement("link"),
		});
		expect(result.mounted).toBe(true);
		expect(result.frontendId).toBe("default");
		expect(root.textContent).toBe("default");
		expect(result.diagnostics.errors.join("\n")).toContain("is not installed");
	});

	it("falls back to default when the active frontend's dynamic import fails", async () => {
		const result = await bootstrapFrontend({
			root,
			emergencyHost: emergency,
			fetchImpl: makeFetch({ activeFrontend: "cyberpunk" }),
			importModule: async (url) => {
				if (url.includes("/cyberpunk/")) throw new Error("chunk load error");
				return mountableModule("default");
			},
			loadStyle: () => document.createElement("link"),
		});
		expect(result.mounted).toBe(true);
		expect(result.frontendId).toBe("default");
		expect(result.diagnostics.attempted).toEqual(["cyberpunk", "default"]);
		expect(result.diagnostics.errors.join("\n")).toContain("chunk load error");
	});

	it("falls back to default when the active frontend throws during mount", async () => {
		const result = await bootstrapFrontend({
			root,
			emergencyHost: emergency,
			fetchImpl: makeFetch({ activeFrontend: "cyberpunk" }),
			importModule: async (url) => {
				if (url.includes("/cyberpunk/")) {
					return {
						mount: () => {
							throw new Error("mount exploded");
						},
					};
				}
				return mountableModule("default");
			},
			loadStyle: () => document.createElement("link"),
		});
		expect(result.mounted).toBe(true);
		expect(result.frontendId).toBe("default");
	});

	it("falls back to default when the active frontend is incompatible", async () => {
		const result = await bootstrapFrontend({
			root,
			emergencyHost: emergency,
			fetchImpl: makeFetch({ activeFrontend: "legacy-runtime" }),
			importModule: async () => mountableModule("default"),
			loadStyle: () => document.createElement("link"),
		});
		expect(result.mounted).toBe(true);
		expect(result.frontendId).toBe("default");
		expect(result.diagnostics.errors.join("\n")).toContain("requires frontend runtime 9");
	});

	it("renders the emergency UI when even the default frontend fails", async () => {
		const result = await bootstrapFrontend({
			root,
			emergencyHost: emergency,
			fetchImpl: makeFetch({ activeFrontend: "cyberpunk" }),
			importModule: async () => {
				throw new Error("all chunks are gone");
			},
			loadStyle: () => document.createElement("link"),
		});
		expect(result.mounted).toBe(false);
		expect(emergency.querySelector("[data-floatctf-bootstrap-error]")).not.toBeNull();
		expect(emergency.textContent).toContain("FloatCTF 界面加载失败");
		expect(emergency.textContent).toContain("all chunks are gone");
	});

	it("renders the emergency UI when the registry is unavailable", async () => {
		const result = await bootstrapFrontend({
			root,
			emergencyHost: emergency,
			fetchImpl: makeFetch({ registryStatus: 500 }),
			importModule: async () => mountableModule("default"),
			loadStyle: () => document.createElement("link"),
		});
		expect(result.mounted).toBe(false);
		expect(emergency.querySelector("[data-floatctf-bootstrap-error]")).not.toBeNull();
	});

	it("uses the default frontend when /api/frontend is unavailable", async () => {
		const result = await bootstrapFrontend({
			root,
			emergencyHost: emergency,
			fetchImpl: makeFetch({ bootstrapStatus: 500 }),
			importModule: async () => mountableModule("default"),
			loadStyle: () => document.createElement("link"),
		});
		expect(result.mounted).toBe(true);
		expect(result.frontendId).toBe("default");
		expect(result.diagnostics.errors.join("\n")).toContain("bootstrap info unavailable");
	});

	describe("?frontend= break-glass override", () => {
		it("honours an installed override id regardless of FRONTEND_ACTIVE", async () => {
			const result = await bootstrapFrontend({
				root,
				emergencyHost: emergency,
				overrideFrontendId: "default",
				fetchImpl: makeFetch({ activeFrontend: "cyberpunk" }),
				importModule: async (url) =>
					url.includes("/default/") ? mountableModule("default") : mountableModule("cyberpunk"),
				loadStyle: () => document.createElement("link"),
			});
			expect(result.mounted).toBe(true);
			expect(result.frontendId).toBe("default");
			expect(result.diagnostics.overrideFrontendId).toBe("default");
			expect(result.diagnostics.attempted).toEqual(["default"]);
		});

		it("ignores an override that is not installed", async () => {
			const result = await bootstrapFrontend({
				root,
				emergencyHost: emergency,
				overrideFrontendId: "attacker-frontend",
				fetchImpl: makeFetch({ activeFrontend: "default" }),
				importModule: async () => mountableModule("default"),
				loadStyle: () => document.createElement("link"),
			});
			expect(result.mounted).toBe(true);
			expect(result.frontendId).toBe("default");
			expect(result.diagnostics.overrideFrontendId).toBeUndefined();
			expect(result.diagnostics.errors.join("\n")).toContain("ignored ?frontend=attacker-frontend");
		});

		it("ignores an unsafe override value", async () => {
			const imported: string[] = [];
			const result = await bootstrapFrontend({
				root,
				emergencyHost: emergency,
				overrideFrontendId: "../../etc/passwd",
				fetchImpl: makeFetch({ activeFrontend: "default" }),
				importModule: async (url) => {
					imported.push(url);
					return mountableModule("default");
				},
				loadStyle: () => document.createElement("link"),
			});
			expect(result.frontendId).toBe("default");
			expect(imported.every((url) => !url.includes(".."))).toBe(true);
		});
	});

	it("injects the registry-declared stylesheet and removes the failed frontend's styles on fallback", async () => {
		const injected: string[] = [];
		const result = await bootstrapFrontend({
			root,
			emergencyHost: emergency,
			fetchImpl: makeFetch({ activeFrontend: "cyberpunk" }),
			importModule: async (url) => {
				if (url.includes("/cyberpunk/")) throw new Error("boom");
				return mountableModule("default");
			},
			loadStyle: (url) => {
				injected.push(url);
				const link = document.createElement("link");
				link.rel = "stylesheet";
				link.href = url;
				document.head.appendChild(link);
				return link;
			},
		});
		expect(result.frontendId).toBe("default");
		expect(injected).toEqual([
			"/__floatctf/frontends/cyberpunk/1.0.0/assets/frontend.css",
			"/__floatctf/frontends/default/1.0.0/assets/frontend.css",
		]);
		const remaining = Array.from(document.head.querySelectorAll("link[rel=stylesheet]")).map(
			(link) => (link as HTMLLinkElement).getAttribute("href"),
		);
		expect(remaining).toEqual(["/__floatctf/frontends/default/1.0.0/assets/frontend.css"]);
	});
});
