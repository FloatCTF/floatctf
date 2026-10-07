/**
 * 本地已安装前端注册表（`registry.json`）。
 *
 * 语义边界（理解这一点比记住字段更重要）：
 * - **注册表描述"装了什么"**：哪些前端 ID、每个 ID 有哪些版本、每个版本的资产入口。
 * - **平台设置 `FRONTEND_ACTIVE` 描述"用哪个"**：只存前端 ID，不存版本。
 * - 每个 ID 有一个**显式** `currentVersion` 指针：安装新版本或回滚都必须显式改它，
 *   绝不按文件名/版本号字典序"猜"当前版本（`9.0.0` vs `10.0.0` 这类排序会随机换前端）。
 *
 * 注册表由 `scripts/frontend.sh` 原子写入（tmp + rename），浏览器只读。
 */

import {
	FRONTEND_REGISTRY_SCHEMA_VERSION,
	FRONTEND_RUNTIME_VERSION,
	API_CONTRACT_VERSION,
} from "./version.js";
import {
	checkRelativeAssetPath,
	isMajorCompatible,
	isSafeFrontendId,
	isValidSemver,
	joinUrlPath,
} from "./paths.js";
import type { FloatCTFFrontendCompatibility } from "./manifest.js";

/** 单个已安装版本。 */
export interface FloatCTFRegistryVersion {
	version: string;
	name: string;
	description?: string;
	author?: string;
	compatibility: FloatCTFFrontendCompatibility;
	entry: string;
	styles: string[];
	installedAt: string;
}

/** 单个前端 ID。 */
export interface FloatCTFRegistryFrontend {
	id: string;
	/** 显式当前版本指针；必须存在于 `versions` 中。 */
	currentVersion: string;
	/** `default` 前端为 true：常规前端管理不得移除。 */
	protected: boolean;
	versions: Record<string, FloatCTFRegistryVersion>;
}

export interface FloatCTFRegistry {
	schemaVersion: number;
	updatedAt: string;
	frontends: Record<string, FloatCTFRegistryFrontend>;
}

export type RegistryParseResult =
	| { ok: true; registry: FloatCTFRegistry }
	| { ok: false; errors: string[] };

function isPlainObject(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseVersionEntry(
	id: string,
	version: string,
	raw: unknown,
	errors: string[],
): FloatCTFRegistryVersion | null {
	const localErrors: string[] = [];
	const push = (message: string) => {
		localErrors.push(message);
		errors.push(message);
	};

	if (!isPlainObject(raw)) {
		push(`registry: frontends.${id}.versions.${version} must be an object`);
		return null;
	}
	if (raw.version !== version) {
		push(
			`registry: frontends.${id}.versions.${version}.version=${JSON.stringify(raw.version)} does not match its key`,
		);
	}
	if (!isValidSemver(version)) {
		push(`registry: frontends.${id}.versions key ${version} is not valid semver`);
	}
	if (typeof raw.name !== "string" || raw.name.trim().length === 0) {
		push(`registry: frontends.${id}.versions.${version}.name must be a non-empty string`);
	}

	const entryCheck = checkRelativeAssetPath(raw.entry);
	if (!entryCheck.ok) {
		push(`registry: frontends.${id}.versions.${version}.entry: ${entryCheck.reason}`);
	}

	const styles: string[] = [];
	if (raw.styles !== undefined) {
		if (!Array.isArray(raw.styles)) {
			push(`registry: frontends.${id}.versions.${version}.styles must be an array`);
		} else {
			raw.styles.forEach((style, index) => {
				const styleCheck = checkRelativeAssetPath(style);
				if (!styleCheck.ok) {
					push(
						`registry: frontends.${id}.versions.${version}.styles[${index}]: ${styleCheck.reason}`,
					);
				} else {
					styles.push(style as string);
				}
			});
		}
	}

	let compatibility: FloatCTFFrontendCompatibility | null = null;
	if (!isPlainObject(raw.compatibility)) {
		push(
			`registry: frontends.${id}.versions.${version}.compatibility must be an object`,
		);
	} else {
		const { frontendRuntime, apiContract, sdk } = raw.compatibility;
		if (typeof frontendRuntime !== "string" || typeof apiContract !== "string") {
			push(
				`registry: frontends.${id}.versions.${version}.compatibility must declare frontendRuntime and apiContract`,
			);
		} else {
			compatibility = {
				frontendRuntime,
				apiContract,
				...(typeof sdk === "string" ? { sdk } : {}),
			};
		}
	}

	if (typeof raw.installedAt !== "string" || raw.installedAt.length === 0) {
		push(`registry: frontends.${id}.versions.${version}.installedAt must be a string`);
	}

	if (localErrors.length > 0 || !compatibility) {
		return null;
	}

	return {
		version,
		name: raw.name as string,
		...(typeof raw.description === "string" ? { description: raw.description } : {}),
		...(typeof raw.author === "string" ? { author: raw.author } : {}),
		compatibility,
		entry: raw.entry as string,
		styles,
		installedAt: raw.installedAt as string,
	};
}

/**
 * 严格解析并校验注册表。任何一项校验失败都返回 `ok:false` 与全部错误
 * （而不是"跳过坏条目继续"——坏注册表必须显式可见，否则会出现
 * "某个前端突然消失"这类无法定位的故障）。
 */
export function parseRegistry(
	input: unknown,
	options: { registrySchemaVersion?: number } = {},
): RegistryParseResult {
	const expectedSchema =
		options.registrySchemaVersion ?? FRONTEND_REGISTRY_SCHEMA_VERSION;

	let raw: unknown = input;
	if (typeof input === "string") {
		try {
			raw = JSON.parse(input);
		} catch (error) {
			return { ok: false, errors: [`registry.json is not valid JSON: ${String(error)}`] };
		}
	}
	if (!isPlainObject(raw)) {
		return { ok: false, errors: ["registry.json must be a JSON object"] };
	}

	const errors: string[] = [];

	if (raw.schemaVersion !== expectedSchema) {
		errors.push(
			`registry.json: unsupported schemaVersion ${JSON.stringify(raw.schemaVersion)} (expected ${expectedSchema})`,
		);
	}
	if (typeof raw.updatedAt !== "string" || raw.updatedAt.length === 0) {
		errors.push("registry.json: updatedAt must be a string");
	}
	if (!isPlainObject(raw.frontends)) {
		errors.push("registry.json: frontends must be an object");
		return { ok: false, errors };
	}

	const frontends: Record<string, FloatCTFRegistryFrontend> = {};

	for (const [id, rawFrontend] of Object.entries(raw.frontends)) {
		if (!isSafeFrontendId(id)) {
			errors.push(`registry.json: frontend id \`${id}\` is not a safe id`);
			continue;
		}
		if (!isPlainObject(rawFrontend)) {
			errors.push(`registry.json: frontends.${id} must be an object`);
			continue;
		}
		if (rawFrontend.id !== undefined && rawFrontend.id !== id) {
			errors.push(
				`registry.json: frontends.${id}.id=${JSON.stringify(rawFrontend.id)} does not match its key`,
			);
		}
		if (!isPlainObject(rawFrontend.versions)) {
			errors.push(`registry.json: frontends.${id}.versions must be an object`);
			continue;
		}

		const versions: Record<string, FloatCTFRegistryVersion> = {};
		for (const [version, rawVersion] of Object.entries(rawFrontend.versions)) {
			const parsed = parseVersionEntry(id, version, rawVersion, errors);
			if (parsed) versions[version] = parsed;
		}
		if (Object.keys(versions).length === 0) {
			errors.push(`registry.json: frontends.${id} has no valid versions`);
			continue;
		}

		const currentVersion = rawFrontend.currentVersion;
		if (typeof currentVersion !== "string" || !(currentVersion in versions)) {
			errors.push(
				`registry.json: frontends.${id}.currentVersion ${JSON.stringify(currentVersion)} is not an installed version`,
			);
			continue;
		}

		frontends[id] = {
			id,
			currentVersion,
			protected: rawFrontend.protected === true,
			versions,
		};
	}

	if (Object.keys(frontends).length === 0) {
		errors.push("registry.json: no valid frontends installed");
	}

	if (errors.length > 0) {
		return { ok: false, errors };
	}

	return {
		ok: true,
		registry: {
			schemaVersion: expectedSchema,
			updatedAt: raw.updatedAt as string,
			frontends,
		},
	};
}

/** 空注册表（首次安装 / 测试基线）。 */
export function emptyRegistry(now: string): FloatCTFRegistry {
	return {
		schemaVersion: FRONTEND_REGISTRY_SCHEMA_VERSION,
		updatedAt: now,
		frontends: {},
	};
}

/**
 * 注册某个前端的某个版本。
 *
 * 与 `scripts/frontend.sh` 的同名语义保持一致：
 * - 前端 ID / 版本格式必须合法
 * - **重复版本直接拒绝**（不静默覆盖已安装的不可变资产）
 * - 仅当该 ID 尚无 `currentVersion`，或调用方要求 `makeCurrent` 时才移动指针
 */
export function registerFrontendVersion(
	registry: FloatCTFRegistry,
	entry: {
		id: string;
		currentVersion?: string;
		protected?: boolean;
		version: FloatCTFRegistryVersion;
	},
	makeCurrent = false,
): { ok: true; registry: FloatCTFRegistry } | { ok: false; error: string } {
	const { id } = entry;
	if (!isSafeFrontendId(id)) {
		return { ok: false, error: `unsafe frontend id: ${id}` };
	}
	if (!isValidSemver(entry.version.version)) {
		return { ok: false, error: `invalid semver version: ${entry.version.version}` };
	}
	const existing = registry.frontends[id];
	if (existing && entry.version.version in existing.versions) {
		return {
			ok: false,
			error: `frontend ${id} version ${entry.version.version} is already installed`,
		};
	}
	const versions = { ...(existing?.versions ?? {}), [entry.version.version]: entry.version };
	const currentVersion =
		makeCurrent || !existing?.currentVersion
			? entry.version.version
			: existing.currentVersion;
	return {
		ok: true,
		registry: {
			...registry,
			frontends: {
				...registry.frontends,
				[id]: {
					id,
					currentVersion,
					protected: entry.protected ?? existing?.protected ?? false,
					versions,
				},
			},
		},
	};
}

// ── 解析（挑选要加载的前端）──────────────────────────────────────────────────

export interface ResolvedFrontend {
	id: string;
	version: string;
	name: string;
	description?: string;
	author?: string;
	compatibility: FloatCTFFrontendCompatibility;
	/** 入口 ESM 的**同源绝对路径**（已用注册表校验过的相对路径拼成）。 */
	entryUrl: string;
	/** 样式表同源绝对路径。 */
	styleUrls: string[];
	/** 制品根目录的同源绝对路径（前端可用于加载自身资产）。 */
	assetBaseUrl: string;
}

export type ResolveFrontendResult =
	| { ok: true; frontend: ResolvedFrontend }
	| { ok: false; errors: string[] };

export interface ResolveFrontendOptions {
	/** 平台设置里的 `FRONTEND_ACTIVE`（前端 ID）。 */
	requestedId: string | null | undefined;
	/** 前端资产挂载前缀，默认 `/__floatctf/frontends`。 */
	frontendBaseUrl: string;
	frontendRuntimeVersion?: string;
	apiContractVersion?: string;
}

/**
 * 从注册表解析要加载的前端。
 *
 * 语义：
 * - `requestedId` 缺失/非法/未安装 → 失败（调用方负责回退到 `default`）
 * - 使用注册表**显式** `currentVersion`，绝不按版本号排序挑选
 * - 逐一校验兼容性（runtime / API contract major）
 * - 已校验的相对路径才拼接为同源 URL（不允许跨源 / 穿越）
 */
export function resolveFrontend(
	registry: FloatCTFRegistry,
	options: ResolveFrontendOptions,
): ResolveFrontendResult {
	const runtimeVersion = options.frontendRuntimeVersion ?? FRONTEND_RUNTIME_VERSION;
	const apiContract = options.apiContractVersion ?? API_CONTRACT_VERSION;
	const baseUrl = options.frontendBaseUrl.replace(/\/+$/, "");

	const requestedId = options.requestedId;
	if (typeof requestedId !== "string" || !isSafeFrontendId(requestedId)) {
		return {
			ok: false,
			errors: [`active frontend id ${JSON.stringify(requestedId)} is not a safe frontend id`],
		};
	}

	const frontend = registry.frontends[requestedId];
	if (!frontend) {
		return { ok: false, errors: [`frontend \`${requestedId}\` is not installed`] };
	}

	const version = frontend.versions[frontend.currentVersion];
	if (!version) {
		return {
			ok: false,
			errors: [
				`frontend \`${requestedId}\` currentVersion ${frontend.currentVersion} is not installed`,
			],
		};
	}

	const errors: string[] = [];
	if (!isMajorCompatible(version.compatibility.frontendRuntime, runtimeVersion)) {
		errors.push(
			`frontend \`${requestedId}\` requires frontend runtime ${version.compatibility.frontendRuntime} (platform runtime is ${runtimeVersion})`,
		);
	}
	if (!isMajorCompatible(version.compatibility.apiContract, apiContract)) {
		errors.push(
			`frontend \`${requestedId}\` requires API contract ${version.compatibility.apiContract} (platform contract is ${apiContract})`,
		);
	}

	const entryCheck = checkRelativeAssetPath(version.entry);
	if (!entryCheck.ok) {
		errors.push(`frontend \`${requestedId}\` entry: ${entryCheck.reason}`);
	}
	for (const [index, style] of version.styles.entries()) {
		const styleCheck = checkRelativeAssetPath(style);
		if (!styleCheck.ok) {
			errors.push(`frontend \`${requestedId}\` styles[${index}]: ${styleCheck.reason}`);
		}
	}

	if (errors.length > 0) return { ok: false, errors };

	const assetBaseUrl = joinUrlPath(baseUrl, requestedId, version.version);
	return {
		ok: true,
		frontend: {
			id: requestedId,
			version: version.version,
			name: version.name,
			...(version.description !== undefined ? { description: version.description } : {}),
			...(version.author !== undefined ? { author: version.author } : {}),
			compatibility: version.compatibility,
			entryUrl: joinUrlPath(assetBaseUrl, version.entry),
			styleUrls: version.styles.map((style) => joinUrlPath(assetBaseUrl, style)),
			assetBaseUrl,
		},
	};
}

/** 已安装前端 ID 列表（用于管理端选择器 / `?frontend=` 白名单）。 */
export function listFrontendIds(registry: FloatCTFRegistry): string[] {
	return Object.keys(registry.frontends).sort();
}
