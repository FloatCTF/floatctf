/**
 * FloatCTF 前端平台契约版本（三个互相独立的版本概念，禁止合并成一个变量）：
 *
 * | 常量 | 含义 | 变更触发条件 |
 * |------|------|--------------|
 * | `FRONTEND_RUNTIME_VERSION` | 前端运行时 / 制品契约的 major | 破坏 `frontend.json` 或 `mount(context)` 契约 |
 * | `API_CONTRACT_VERSION` | 对外 HTTP API 契约的 major | 破坏前端可见的 API（删字段 / 改语义） |
 * | `FRONTEND_REGISTRY_SCHEMA_VERSION` | 本地注册表 `registry.json` 的 schema major | 注册表结构不兼容变更 |
 *
 * 平台自身包版本（`1.0.0`）与以上三者无关，不要互相推导。
 */

/** 前端运行时契约 major；前端 `compatibility.frontendRuntime` 必须与之相等。 */
export const FRONTEND_RUNTIME_VERSION = "1";

/** 对外 HTTP API 契约 major；前端 `compatibility.apiContract` 必须与之相等。 */
export const API_CONTRACT_VERSION = "1";

/** 本地前端注册表 `registry.json` 的 schema 版本。 */
export const FRONTEND_REGISTRY_SCHEMA_VERSION = 1;

/** 单个前端制品 `frontend.json`（manifest 文档）的 schema 版本。 */
export const FRONTEND_MANIFEST_SCHEMA_VERSION = 1;

/** 当前前端运行时版本（含 minor/patch，仅用于诊断展示）。 */
export const FRONTEND_RUNTIME_FULL_VERSION = "1.0.0";

/** 生产环境前端资产默认挂载前缀（与 Caddy 配置一致）。 */
export const DEFAULT_FRONTEND_BASE_URL = "/__floatctf/frontends";

/** 默认注册表 URL（同源静态文件，非永久缓存）。 */
export const DEFAULT_REGISTRY_URL = `${DEFAULT_FRONTEND_BASE_URL}/registry.json`;

/** 默认 API base URL。 */
export const DEFAULT_API_BASE_URL = "/api";

/** 兜底前端 ID：始终随平台发布、不可删除。 */
export const DEFAULT_FRONTEND_ID = "default";
