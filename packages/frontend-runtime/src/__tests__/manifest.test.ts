import { describe, expect, it } from "vitest";

import { parseFrontendManifest } from "../manifest";

function validManifest(overrides: Record<string, unknown> = {}) {
	return {
		schemaVersion: 1,
		id: "cyberpunk",
		name: "Cyberpunk Frontend",
		version: "1.2.0",
		compatibility: { frontendRuntime: "1", apiContract: "1" },
		entry: "assets/frontend.js",
		styles: ["assets/frontend.css"],
		...overrides,
	};
}

describe("parseFrontendManifest", () => {
	it("accepts a valid manifest", () => {
		const result = parseFrontendManifest(validManifest());
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		expect(result.manifest.id).toBe("cyberpunk");
		expect(result.manifest.entry).toBe("assets/frontend.js");
		expect(result.manifest.styles).toEqual(["assets/frontend.css"]);
	});

	it("accepts a JSON string input", () => {
		const result = parseFrontendManifest(JSON.stringify(validManifest()));
		expect(result.ok).toBe(true);
	});

	it("rejects an unsupported schemaVersion", () => {
		const result = parseFrontendManifest(validManifest({ schemaVersion: 2 }));
		expect(result.ok).toBe(false);
		if (result.ok) return;
		expect(result.errors.join("\n")).toContain("unsupported schemaVersion");
	});

	it("rejects a missing schemaVersion", () => {
		const manifest = validManifest();
		// biome-ignore lint/performance/noDelete: 测试需要构造缺字段输入
		delete (manifest as Record<string, unknown>).schemaVersion;
		const result = parseFrontendManifest(manifest);
		expect(result.ok).toBe(false);
	});

	it.each([
		"Cyberpunk",
		"../evil",
		"a/b",
		"-leading-dash",
		"",
		"with space",
		"UPPER",
	])("rejects unsafe frontend id %j", (id) => {
		const result = parseFrontendManifest(validManifest({ id }));
		expect(result.ok).toBe(false);
	});

	it.each(["1.0", "v1.0.0", "1", "not-a-version", "01.2.3"])(
		"rejects invalid semver version %j",
		(version) => {
			const result = parseFrontendManifest(validManifest({ version }));
			expect(result.ok).toBe(false);
		},
	);

	it("accepts prerelease semver", () => {
		const result = parseFrontendManifest(validManifest({ version: "1.2.0-rc.1" }));
		expect(result.ok).toBe(true);
	});

	it.each([
		"../outside.js",
		"assets/../../outside.js",
		"/absolute.js",
		"https://cdn.example.com/frontend.js",
		"data:text/javascript,alert(1)",
		"assets\\windows.js",
		"./assets/frontend.js",
		"assets//frontend.js",
	])("rejects traversal / absolute / scheme entry %j", (entry) => {
		const result = parseFrontendManifest(validManifest({ entry }));
		expect(result.ok).toBe(false);
		if (result.ok) return;
		expect(result.errors.join("\n")).toContain("entry");
	});

	it.each([
		"../outside.css",
		"/absolute.css",
		"https://cdn.example.com/style.css",
		"assets/../../outside.css",
	])("rejects traversal / absolute style %j", (style) => {
		const result = parseFrontendManifest(validManifest({ styles: [style] }));
		expect(result.ok).toBe(false);
		if (result.ok) return;
		expect(result.errors.join("\n")).toContain("styles[0]");
	});

	it("rejects an incompatible frontend runtime major", () => {
		const result = parseFrontendManifest(
			validManifest({
				compatibility: { frontendRuntime: "2", apiContract: "1" },
			}),
		);
		expect(result.ok).toBe(false);
		if (result.ok) return;
		expect(result.errors.join("\n")).toContain("compatibility.frontendRuntime");
	});

	it("rejects an incompatible API contract major", () => {
		const result = parseFrontendManifest(
			validManifest({
				compatibility: { frontendRuntime: "1", apiContract: "3" },
			}),
		);
		expect(result.ok).toBe(false);
		if (result.ok) return;
		expect(result.errors.join("\n")).toContain("compatibility.apiContract");
	});

	it("rejects semver-range style compatibility constraints (v1 is integer major only)", () => {
		const result = parseFrontendManifest(
			validManifest({
				compatibility: { frontendRuntime: ">=1 <2", apiContract: "1" },
			}),
		);
		expect(result.ok).toBe(false);
	});

	it("rejects unknown top-level fields (fail-closed)", () => {
		const result = parseFrontendManifest(validManifest({ sidebar: true }));
		expect(result.ok).toBe(false);
		if (result.ok) return;
		expect(result.errors.join("\n")).toContain("unknown field");
	});

	it("treats compatibility.sdk as informational", () => {
		const result = parseFrontendManifest(
			validManifest({
				compatibility: { frontendRuntime: "1", apiContract: "1", sdk: "1.0.0" },
			}),
		);
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		expect(result.warnings.join("\n")).toContain("informational");
	});

	it("rejects malformed JSON strings", () => {
		const result = parseFrontendManifest("{not json");
		expect(result.ok).toBe(false);
	});

	it("rejects a non-object manifest", () => {
		expect(parseFrontendManifest("[]").ok).toBe(false);
		expect(parseFrontendManifest("null").ok).toBe(false);
	});

	it("validates against injected contract versions", () => {
		const result = parseFrontendManifest(
			validManifest({ compatibility: { frontendRuntime: "2", apiContract: "2" } }),
			{ frontendRuntimeVersion: "2", apiContractVersion: "2" },
		);
		expect(result.ok).toBe(true);
	});
});
