/**
 * FloatCTF 前端模块契约 —— 一个前端制品默认导出的东西。
 *
 * ```ts
 * // frontend 制品入口（任意框架）
 * import type { FloatCTFFrontendModule } from "@floatctf/frontend-runtime";
 *
 * export function mount(context: FloatCTFMountContext) {
 *   // 创建自己的框架/路由/状态/样式，渲染进 context.root
 *   return () => { /@ unmount @/ };   // 可选：返回清理函数
 * }
 * ```
 */

/** `mount()` 收到的平台能力上下文。刻意保持极小。 */
export interface FloatCTFMountContext {
	/** 前端渲染的宿主元素（bootstrap 已确保为空）。 */
	root: HTMLElement;
	/** API base URL，例如 `/api`（同源）。前端自行决定如何调用。 */
	apiBaseUrl: string;
	/** 本前端制品根目录的同源绝对路径（用于加载自己的图片/字体等）。 */
	assetBaseUrl: string;
	/** 当前前端 ID 与版本（诊断/日志用）。 */
	frontendId: string;
	frontendVersion: string;
	/** 平台版本（`/api/frontend` 返回，纯展示/诊断用）。 */
	platformVersion: string;
	/** 对外 HTTP API 契约 major。 */
	apiContractVersion: string;
	/** 前端运行时契约 major。 */
	frontendRuntimeVersion: string;
	/** 真实且稳定的平台能力标记（列表内元素由平台声明，前端按需探测）。 */
	capabilities: readonly string[];
}

/** 前端 mount 的可选返回值：清理函数。 */
export type FloatCTFFrontendUnmount = () => void;

/** 前端制品入口模块的默认导出对象。 */
export interface FloatCTFFrontendModule {
	/** 可选：制品内自述的 ID/版本（仅用于诊断日志，平台以注册表为准）。 */
	manifest?: { id?: string; version?: string };
	/** 把前端挂载到 `context.root`。允许 async；抛错会触发 bootstrap 回退。 */
	mount(context: FloatCTFMountContext): void | FloatCTFFrontendUnmount | Promise<void | FloatCTFFrontendUnmount>;
}

/** 动态 `import()` 到的模块形状（可能是 `default`，也可能是命名导出 `mount`）。 */
export type FloatCTFFrontendImport = Partial<FloatCTFFrontendModule> & {
	default?: Partial<FloatCTFFrontendModule> | FloatCTFFrontendModule;
};

/**
 * 从动态 import 结果中取出可用的 module。
 * 容忍 `export default { mount }`、`export default function mount` 与 `export function mount`。
 */
export function normalizeFrontendModule(
	imported: unknown,
): FloatCTFFrontendModule | null {
	if (typeof imported !== "object" || imported === null) return null;
	const record = imported as Record<string, unknown>;

	const candidates: unknown[] = [];
	if (typeof record.mount === "function") {
		candidates.push({ mount: record.mount });
	}
	const defaultExport = record.default;
	if (typeof defaultExport === "function") {
		candidates.push({ mount: defaultExport });
	} else if (typeof defaultExport === "object" && defaultExport !== null) {
		const defaultRecord = defaultExport as Record<string, unknown>;
		if (typeof defaultRecord.mount === "function") {
			candidates.push({
				mount: defaultRecord.mount,
				...(typeof defaultRecord.manifest === "object" && defaultRecord.manifest !== null
					? { manifest: defaultRecord.manifest as FloatCTFFrontendModule["manifest"] }
					: {}),
			});
		}
	}

	for (const candidate of candidates) {
		if (typeof (candidate as FloatCTFFrontendModule).mount === "function") {
			return candidate as FloatCTFFrontendModule;
		}
	}
	return null;
}
