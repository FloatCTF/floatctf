/**
 * 前端制品 manifest（`frontend.json`）—— **故意保持很小**。
 *
 * 它只声明"这是什么、吃什么版本、从哪加载"，**不声明**路由、页面、框架、
 * 布局、登录页或任何 UI 结构。UI 结构完全属于前端实现自己。
 */

import {
	FRONTEND_MANIFEST_SCHEMA_VERSION,
	FRONTEND_RUNTIME_VERSION,
	API_CONTRACT_VERSION,
} from "./version.js";
import {
	type PathCheckResult,
	checkRelativeAssetPath,
	isMajorCompatible,
	isSafeFrontendId,
	isValidSemver,
} from "./paths.js";

/** 兼容性声明：整数 major 约束（v1 语义，见 paths.ts 的 parseMajorConstraint）。 */
export interface FloatCTFFrontendCompatibility {
	/** 期望的前端运行时契约 major（如 `"1"`）。 */
	frontendRuntime: string;
	/** 期望的对外 HTTP API 契约 major（如 `"1"`）。 */
	apiContract: string;
	/** 可选：构建时使用的 `@floatctf/sdk` 版本（**信息性**，运行时不强制）。 */
	sdk?: string;
}

/** 一个可安装的 FloatCTF 前端制品。 */
export interface FloatCTFFrontendManifest {
	schemaVersion: number;
	id: string;
	name: string;
	version: string;
	description?: string;
	author?: string;
	compatibility: FloatCTFFrontendCompatibility;
	/** 相对本制品根目录的 ESM 入口（必须导出 `mount`）。 */
	entry: string;
	/** 相对本制品根目录的样式文件（bootstrap 负责注入 `<link>`）。 */
	styles?: string[];
}

export type ManifestParseResult =
	| { ok: true; manifest: FloatCTFFrontendManifest; warnings: string[] }
	| { ok: false; errors: string[] };

function isPlainObject(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function collectPathErrors(
	label: string,
	value: unknown,
	errors: string[],
): void {
	const result: PathCheckResult = checkRelativeAssetPath(value);
	if (!result.ok) {
		errors.push(`${label}: ${result.reason}`);
	}
}

const MANIFEST_KEYS = new Set([
	"schemaVersion",
	"id",
	"name",
	"version",
	"description",
	"author",
	"compatibility",
	"entry",
	"styles",
]);

/**
 * 严格解析并校验 `frontend.json`。
 *
 * 未知字段直接拒绝（fail-closed）：制品格式是安全边界，静默忽略未知字段
 * 会让"看起来装了但实际少了一件事"的故障极难排查。
 *
 * @param input 已 `JSON.parse` 的对象；也可传原始字符串。
 * @param options 可覆盖当前平台契约版本（测试注入用）。
 */
export function parseFrontendManifest(
	input: unknown,
	options: {
		manifestSchemaVersion?: number;
		frontendRuntimeVersion?: string;
		apiContractVersion?: string;
	} = {},
): ManifestParseResult {
	const expectedSchema =
		options.manifestSchemaVersion ?? FRONTEND_MANIFEST_SCHEMA_VERSION;
	const runtimeVersion =
		options.frontendRuntimeVersion ?? FRONTEND_RUNTIME_VERSION;
	const apiContract = options.apiContractVersion ?? API_CONTRACT_VERSION;

	let raw: unknown = input;
	if (typeof input === "string") {
		try {
			raw = JSON.parse(input);
		} catch (error) {
			return {
				ok: false,
				errors: [`frontend.json is not valid JSON: ${String(error)}`],
			};
		}
	}

	if (!isPlainObject(raw)) {
		return { ok: false, errors: ["frontend.json must be a JSON object"] };
	}

	const errors: string[] = [];
	const warnings: string[] = [];

	for (const key of Object.keys(raw)) {
		if (!MANIFEST_KEYS.has(key)) {
			errors.push(`frontend.json: unknown field \`${key}\``);
		}
	}

	// ── schemaVersion ──
	if (raw.schemaVersion !== expectedSchema) {
		errors.push(
			`frontend.json: unsupported schemaVersion ${JSON.stringify(raw.schemaVersion)} (expected ${expectedSchema})`,
		);
	}

	// ── id ──
	if (!isSafeFrontendId(raw.id)) {
		errors.push(
			"frontend.json: id must match [a-z0-9][a-z0-9._-]* (max 64 chars)",
		);
	}

	// ── name / version ──
	if (typeof raw.name !== "string" || raw.name.trim().length === 0) {
		errors.push("frontend.json: name must be a non-empty string");
	} else if (raw.name.length > 128) {
		errors.push("frontend.json: name must be at most 128 characters");
	}

	if (!isValidSemver(raw.version)) {
		errors.push("frontend.json: version must be a valid semver string");
	}

	if (raw.description !== undefined) {
		if (typeof raw.description !== "string" || raw.description.length > 1024) {
			errors.push("frontend.json: description must be a string (max 1024)");
		}
	}
	if (raw.author !== undefined) {
		if (typeof raw.author !== "string" || raw.author.length > 256) {
			errors.push("frontend.json: author must be a string (max 256)");
		}
	}

	// ── compatibility ──
	let compatibility: FloatCTFFrontendCompatibility | null = null;
	if (!isPlainObject(raw.compatibility)) {
		errors.push("frontend.json: compatibility must be an object");
	} else {
		const compat = raw.compatibility;
		for (const key of Object.keys(compat)) {
			if (!["frontendRuntime", "apiContract", "sdk"].includes(key)) {
				errors.push(`frontend.json: compatibility has unknown field \`${key}\``);
			}
		}
		if (!isMajorCompatible(compat.frontendRuntime, runtimeVersion)) {
			errors.push(
				`frontend.json: compatibility.frontendRuntime ${JSON.stringify(compat.frontendRuntime)} is incompatible with runtime major ${runtimeVersion}`,
			);
		}
		if (!isMajorCompatible(compat.apiContract, apiContract)) {
			errors.push(
				`frontend.json: compatibility.apiContract ${JSON.stringify(compat.apiContract)} is incompatible with API contract major ${apiContract}`,
			);
		}
		if (compat.sdk !== undefined && typeof compat.sdk !== "string") {
			errors.push("frontend.json: compatibility.sdk must be a string when present");
		} else if (typeof compat.sdk === "string" && compat.sdk.length > 128) {
			errors.push("frontend.json: compatibility.sdk must be at most 128 characters");
		} else if (typeof compat.sdk === "string") {
			warnings.push(
				`compatibility.sdk=${compat.sdk} is informational only (frontends bundle their own dependencies)`,
			);
		}
		if (
			typeof compat.frontendRuntime === "string" &&
			typeof compat.apiContract === "string"
		) {
			compatibility = {
				frontendRuntime: compat.frontendRuntime,
				apiContract: compat.apiContract,
				...(typeof compat.sdk === "string" ? { sdk: compat.sdk } : {}),
			};
		}
	}

	// ── entry / styles ──
	collectPathErrors("frontend.json: entry", raw.entry, errors);

	if (raw.styles !== undefined) {
		if (!Array.isArray(raw.styles)) {
			errors.push("frontend.json: styles must be an array when present");
		} else {
			if (raw.styles.length > 16) {
				errors.push("frontend.json: styles must contain at most 16 entries");
			}
			raw.styles.forEach((style, index) => {
				collectPathErrors(`frontend.json: styles[${index}]`, style, errors);
			});
		}
	}

	if (errors.length > 0) {
		return { ok: false, errors };
	}

	return {
		ok: true,
		warnings,
		manifest: {
			schemaVersion: expectedSchema,
			id: raw.id as string,
			name: raw.name as string,
			version: raw.version as string,
			...(typeof raw.description === "string"
				? { description: raw.description }
				: {}),
			...(typeof raw.author === "string" ? { author: raw.author } : {}),
			compatibility: compatibility as FloatCTFFrontendCompatibility,
			entry: raw.entry as string,
			...(Array.isArray(raw.styles) ? { styles: raw.styles as string[] } : {}),
		},
	};
}

/** manifest 的兼容性摘要（用于诊断/管理端展示，不含任何敏感信息）。 */
export function describeManifestCompatibility(
	manifest: FloatCTFFrontendManifest,
): string {
	const { frontendRuntime, apiContract } = manifest.compatibility;
	return `runtime ${frontendRuntime} / api ${apiContract}`;
}
