/**
 * 制品入口 —— 实现 FloatCTF 前端运行时契约（`mount(context)`）。
 *
 * bootstrap（`@floatctf/frontend-runtime`）只负责解析注册表、注入样式、
 * 动态 import 本模块并调用 `mount`；其余（React、路由、数据层、设计系统、页面）
 * 全部由本前端自己拥有。
 *
 * 与 Default Frontend **无任何代码依赖**：本文件只 import 公共包与自身源码。
 */
import type {
	FloatCTFFrontendModule,
	FloatCTFMountContext,
} from "@floatctf/frontend-runtime";

// 样式：dev 由 Vite 注入；生产构建抽成单文件 CSS，由 bootstrap 按 frontend.json 注入。
// `globals.css` 内部会 `@import "animal-island-ui/style"`，且必须在最前。
import "~/styles/globals.css";

import sourceManifest from "../floatctf.frontend.json" with { type: "json" };
import { mountApp } from "./app/mountApp";

export function mount(context: FloatCTFMountContext): () => void {
	return mountApp(context);
}

/** 制品自述（仅诊断用；平台以本地注册表为准）。 */
export const manifest = {
	id: sourceManifest.id,
	version: sourceManifest.version,
};

const frontendModule: FloatCTFFrontendModule = { manifest, mount };
export default frontendModule;
