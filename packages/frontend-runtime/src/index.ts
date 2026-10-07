/**
 * `@floatctf/frontend-runtime` —— FloatCTF 可插拔前端的**制品 / 运行时契约**。
 *
 * 这个包**刻意保持很小**，并且**与框架无关**：
 * - 不依赖 React / Vue / Svelte / Primer / 任何路由或状态库
 * - 不规定路由、页面、布局、登录页、管理页
 * - 只规定：制品长什么样（`frontend.json`）、注册表长什么样（`registry.json`）、
 *   以及 `mount(context)` 收到什么、bootstrap 如何加载与回退
 *
 * 三份文档：
 * - `docs/frontend/ARCHITECTURE.md` —— 平台架构与信任模型
 * - `docs/frontend/DEVELOPING.md` —— 外部仓库开发流程
 * - `docs/frontend/ARTIFACT.md` —— 制品 / manifest / 注册表字段与版本策略
 */

export {
	FRONTEND_RUNTIME_VERSION,
	FRONTEND_RUNTIME_FULL_VERSION,
	FRONTEND_MANIFEST_SCHEMA_VERSION,
	FRONTEND_REGISTRY_SCHEMA_VERSION,
	API_CONTRACT_VERSION,
	DEFAULT_FRONTEND_BASE_URL,
	DEFAULT_REGISTRY_URL,
	DEFAULT_API_BASE_URL,
	DEFAULT_FRONTEND_ID,
} from "./version.js";

export {
	FRONTEND_ID_PATTERN,
	FRONTEND_ID_MAX_LENGTH,
	type PathCheckResult,
	checkRelativeAssetPath,
	isMajorCompatible,
	isSafeFrontendId,
	isValidSemver,
	joinUrlPath,
	parseMajorConstraint,
} from "./paths.js";

export {
	type FloatCTFFrontendCompatibility,
	type FloatCTFFrontendManifest,
	type ManifestParseResult,
	parseFrontendManifest,
	describeManifestCompatibility,
} from "./manifest.js";

export {
	type FloatCTFRegistry,
	type FloatCTFRegistryFrontend,
	type FloatCTFRegistryVersion,
	type RegistryParseResult,
	type ResolveFrontendOptions,
	type ResolveFrontendResult,
	type ResolvedFrontend,
	emptyRegistry,
	listFrontendIds,
	parseRegistry,
	registerFrontendVersion,
	resolveFrontend,
} from "./registry.js";

export {
	type FloatCTFFrontendImport,
	type FloatCTFFrontendModule,
	type FloatCTFFrontendUnmount,
	type FloatCTFMountContext,
	normalizeFrontendModule,
} from "./module.js";

export {
	type BootstrapDiagnostics,
	renderBootstrapEmergencyUi,
} from "./emergency.js";

export {
	type BootstrapFrontendOptions,
	type BootstrapFrontendResult,
	type FloatCTFBootstrapInfo,
	bootstrapFrontend,
	parseBootstrapInfo,
} from "./bootstrap.js";
