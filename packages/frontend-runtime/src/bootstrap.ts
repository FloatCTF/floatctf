/**
 * FloatCTF 前端 bootstrap 加载器（框架无关，**不依赖 React**）。
 *
 * 职责边界（与 apps/web 分工）：
 * - `apps/web` 是极薄的宿主：读 DOM/URL、调用本函数、渲染兜底 UI 的容器。
 * - 本函数负责：取公开 bootstrap 信息 → 读本地注册表 → 解析 → 兼容性校验 →
 *   注入样式 → 动态 import 同源 ESM → `mount(context)` → 失败回退 → 兜底 UI。
 *
 * 安全不变量：
 * - **永不** import 任意远端 URL：只 import 由"已校验的相对路径 + 同源前缀"拼出的地址。
 * - `?frontend=` 只接受**注册表中已安装**的安全 ID，绝不直接拼进路径。
 * - 兜底 UI 只用 `textContent`，不把外部字符串当 HTML。
 */

import {
	DEFAULT_API_BASE_URL,
	DEFAULT_FRONTEND_BASE_URL,
	DEFAULT_FRONTEND_ID,
	DEFAULT_REGISTRY_URL,
	API_CONTRACT_VERSION,
	FRONTEND_RUNTIME_VERSION,
} from "./version.js";
import { isSafeFrontendId } from "./paths.js";
import {
	type FloatCTFRegistry,
	type ResolvedFrontend,
	parseRegistry,
	resolveFrontend,
} from "./registry.js";
import {
	type FloatCTFFrontendImport,
	type FloatCTFFrontendModule,
	normalizeFrontendModule,
} from "./module.js";
import {
	type BootstrapDiagnostics,
	renderBootstrapEmergencyUi,
} from "./emergency.js";

/** `GET /api/frontend` 的响应（**仅**安全 bootstrap 元数据）。 */
export interface FloatCTFBootstrapInfo {
	active_frontend: string;
	platform_version: string;
	api_contract_version: string;
	frontend_runtime_version: string;
	capabilities: string[];
}

export interface BootstrapFrontendOptions {
	/** 前端渲染宿主元素（会被清空）。 */
	root: HTMLElement;
	/** API base URL，默认 `/api`。 */
	apiBaseUrl?: string;
	/** 注册表 URL，默认 `/__floatctf/frontends/registry.json`。 */
	registryUrl?: string;
	/** 前端资产前缀，默认 `/__floatctf/frontends`。 */
	frontendBaseUrl?: string;
	/** 兜底 UI 宿主，默认 `document.body`。 */
	emergencyHost?: HTMLElement;
	/**
	 * 浏览器本地紧急覆盖（`?frontend=<id>`）。
	 * 只接受注册表里已安装的安全 ID；其他值一律忽略并记入诊断。
	 */
	overrideFrontendId?: string | null;
	/** 回退用的前端 ID，默认 `default`。 */
	fallbackFrontendId?: string;
	/** 注入依赖（测试用）。 */
	fetchImpl?: typeof fetch;
	/** 动态 import 实现（测试用）。 */
	importModule?: (url: string) => Promise<unknown>;
	/** 样式注入实现（测试用）；返回该 link 元素便于回退时移除。 */
	loadStyle?: (url: string, doc: Document) => HTMLElement;
	/** 诊断回调（测试/日志用）。 */
	onDiagnostics?: (diagnostics: BootstrapDiagnostics) => void;
}

export interface BootstrapFrontendResult {
	mounted: boolean;
	frontendId?: string;
	frontendVersion?: string;
	diagnostics: BootstrapDiagnostics;
}

const STYLE_ATTR = "data-floatctf-frontend-style";

function defaultImportModule(url: string): Promise<unknown> {
	// 保持真正的运行时动态 import：URL 由已校验的注册表推导，绝不静态打包。
	return import(/* @vite-ignore */ url);
}

function defaultLoadStyle(url: string, doc: Document): HTMLElement {
	const link = doc.createElement("link");
	link.rel = "stylesheet";
	link.href = url;
	link.setAttribute(STYLE_ATTR, "true");
	doc.head.appendChild(link);
	return link;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null;
}

/** 解析 `GET /api/frontend` 响应；未知/缺失字段一律回落到安全默认值。 */
export function parseBootstrapInfo(input: unknown): FloatCTFBootstrapInfo {
	const fallback: FloatCTFBootstrapInfo = {
		active_frontend: DEFAULT_FRONTEND_ID,
		platform_version: "",
		api_contract_version: API_CONTRACT_VERSION,
		frontend_runtime_version: FRONTEND_RUNTIME_VERSION,
		capabilities: [],
	};
	if (!isRecord(input)) return fallback;
	return {
		active_frontend:
			typeof input.active_frontend === "string" ? input.active_frontend : DEFAULT_FRONTEND_ID,
		platform_version: typeof input.platform_version === "string" ? input.platform_version : "",
		api_contract_version:
			typeof input.api_contract_version === "string"
				? input.api_contract_version
				: API_CONTRACT_VERSION,
		frontend_runtime_version:
			typeof input.frontend_runtime_version === "string"
				? input.frontend_runtime_version
				: FRONTEND_RUNTIME_VERSION,
		capabilities: Array.isArray(input.capabilities)
			? input.capabilities.filter((c): c is string => typeof c === "string")
			: [],
	};
}

/** 组装一次挂载尝试的结果。 */
interface AttemptOutcome {
	module: FloatCTFFrontendModule;
	frontend: ResolvedFrontend;
	styles: HTMLElement[];
}

/**
 * 执行完整 bootstrap 流程。**永不抛错**：任何失败都会走回退链，
 * 最终失败时渲染兜底 UI 并返回 `mounted: false`。
 */
export async function bootstrapFrontend(
	options: BootstrapFrontendOptions,
): Promise<BootstrapFrontendResult> {
	const doc = options.root.ownerDocument;
	const fetchImpl = options.fetchImpl ?? globalThis.fetch?.bind(globalThis);
	const importModule = options.importModule ?? defaultImportModule;
	const loadStyle = options.loadStyle ?? defaultLoadStyle;
	const apiBaseUrl = (options.apiBaseUrl ?? DEFAULT_API_BASE_URL).replace(/\/+$/, "");
	const registryUrl = options.registryUrl ?? DEFAULT_REGISTRY_URL;
	const frontendBaseUrl = options.frontendBaseUrl ?? DEFAULT_FRONTEND_BASE_URL;
	const fallbackFrontendId = options.fallbackFrontendId ?? DEFAULT_FRONTEND_ID;

	const diagnostics: BootstrapDiagnostics = {
		attempted: [],
		errors: [],
	};
	const record = (message: string) => {
		diagnostics.errors.push(message);
	};

	// ── 1. 公开 bootstrap 信息（未登录也必须可用：登录 UX 属于前端本身）──
	let info: FloatCTFBootstrapInfo | null = null;
	if (fetchImpl) {
		try {
			const response = await fetchImpl(`${apiBaseUrl}/frontend`, {
				method: "GET",
				headers: { Accept: "application/json" },
				credentials: "same-origin",
			});
			if (!response.ok) {
				throw new Error(`GET ${apiBaseUrl}/frontend → HTTP ${response.status}`);
			}
			const body: unknown = await response.json();
			// 后端统一响应包装 {code,message,data}；同时容忍直接返回裸对象。
			const payload = isRecord(body) && "data" in body ? body.data : body;
			info = parseBootstrapInfo(payload);
		} catch (error) {
			record(`bootstrap info unavailable: ${describeError(error)}`);
		}
	} else {
		record("bootstrap info unavailable: fetch is not available");
	}

	diagnostics.platformVersion = info?.platform_version || undefined;
	diagnostics.apiContractVersion = info?.api_contract_version ?? API_CONTRACT_VERSION;
	diagnostics.frontendRuntimeVersion =
		info?.frontend_runtime_version ?? FRONTEND_RUNTIME_VERSION;
	diagnostics.activeFrontendId = info?.active_frontend ?? null;

	// ── 2. 本地注册表 ──
	let registry: FloatCTFRegistry | null = null;
	if (fetchImpl) {
		try {
			const response = await fetchImpl(registryUrl, {
				method: "GET",
				headers: { Accept: "application/json" },
				// 注册表/引导元数据不得被永久缓存。
				cache: "no-store",
				credentials: "same-origin",
			});
			if (!response.ok) {
				throw new Error(`GET ${registryUrl} → HTTP ${response.status}`);
			}
			const parsed = parseRegistry(await response.json());
			if (!parsed.ok) {
				throw new Error(parsed.errors.join("; "));
			}
			registry = parsed.registry;
		} catch (error) {
			record(`frontend registry unavailable: ${describeError(error)}`);
		}
	} else {
		record("frontend registry unavailable: fetch is not available");
	}

	// ── 3. 决定候选顺序 ──
	const candidates: string[] = [];
	const requestedOverride = options.overrideFrontendId ?? null;
	if (requestedOverride !== null && requestedOverride !== undefined) {
		if (!isSafeFrontendId(requestedOverride)) {
			record(`ignored unsafe ?frontend value: ${JSON.stringify(requestedOverride)}`);
		} else if (registry && !(requestedOverride in registry.frontends)) {
			record(`ignored ?frontend=${requestedOverride}: not installed`);
		} else {
			diagnostics.overrideFrontendId = requestedOverride;
			candidates.push(requestedOverride);
		}
	}
	const activeId = info?.active_frontend ?? null;
	if (activeId && isSafeFrontendId(activeId) && !candidates.includes(activeId)) {
		candidates.push(activeId);
	} else if (activeId && !isSafeFrontendId(activeId)) {
		record(`active frontend id ${JSON.stringify(activeId)} is not a safe frontend id`);
	}
	if (!candidates.includes(fallbackFrontendId)) {
		candidates.push(fallbackFrontendId);
	}

	// ── 4. 逐个尝试 ──
	for (const candidateId of candidates) {
		const attempt = await attemptMount(candidateId);
		diagnostics.attempted.push(candidateId);
		if (attempt.ok) {
			options.onDiagnostics?.(diagnostics);
			return {
				mounted: true,
				frontendId: attempt.frontend.id,
				frontendVersion: attempt.frontend.version,
				diagnostics,
			};
		}
		record(`${candidateId}: ${attempt.error}`);
	}

	// ── 5. 连默认前端都失败 → 兜底 UI ──
	options.onDiagnostics?.(diagnostics);
	if (isRecord(options.emergencyHost) || options.emergencyHost) {
		renderBootstrapEmergencyUi(options.emergencyHost as HTMLElement, diagnostics);
	} else if (doc.body) {
		renderBootstrapEmergencyUi(doc.body, diagnostics);
	}
	return { mounted: false, diagnostics };

	async function attemptMount(
		frontendId: string,
	): Promise<({ ok: true } & AttemptOutcome) | { ok: false; error: string }> {
		if (!registry) {
			return { ok: false, error: "frontend registry is unavailable" };
		}
		const resolved = resolveFrontend(registry, {
			requestedId: frontendId,
			frontendBaseUrl,
			frontendRuntimeVersion: info?.frontend_runtime_version,
			apiContractVersion: info?.api_contract_version,
		});
		if (!resolved.ok) {
			return { ok: false, error: resolved.errors.join("; ") };
		}
		const frontend = resolved.frontend;

		const styles: HTMLElement[] = [];
		try {
			for (const styleUrl of frontend.styleUrls) {
				styles.push(loadStyle(styleUrl, doc));
			}
			const imported = await importModule(frontend.entryUrl);
			const module = normalizeFrontendModule(imported as FloatCTFFrontendImport);
			if (!module) {
				throw new Error(`entry did not export mount(): ${frontend.entryUrl}`);
			}
			// 清空宿主（上一次尝试可能留下半渲染的 DOM）。
			options.root.textContent = "";
			await module.mount({
				root: options.root,
				apiBaseUrl,
				assetBaseUrl: frontend.assetBaseUrl,
				frontendId: frontend.id,
				frontendVersion: frontend.version,
				platformVersion: info?.platform_version ?? "",
				apiContractVersion: info?.api_contract_version ?? API_CONTRACT_VERSION,
				frontendRuntimeVersion:
					info?.frontend_runtime_version ?? FRONTEND_RUNTIME_VERSION,
				capabilities: info?.capabilities ?? [],
			});
			// 前端可以返回清理函数；bootstrap 每次页面加载只挂载一次，因此不保留它
			// （整页卸载由浏览器负责）。保留契约是为了将来支持热切换。
			return { ok: true, module, frontend, styles };
		} catch (error) {
			for (const style of styles) style.remove();
			return { ok: false, error: describeError(error) };
		}
	}
}

function describeError(error: unknown): string {
	if (error instanceof Error) return error.message;
	return String(error);
}
