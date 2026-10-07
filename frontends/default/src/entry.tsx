/**
 * Default Frontend 的制品入口 —— 实现 FloatCTF 前端运行时契约。
 *
 * bootstrap（`@floatctf/frontend-runtime`）只做三件事：解析注册表、注入样式、
 * 动态 import 这个模块并调用 `mount(context)`。**其余全部在这里**：React、Router、
 * TanStack Query、Primer、Tailwind、styled-components、页面与导航。
 *
 * 与迁移前的 `apps/web/src/main.tsx` 相比，唯一的语义变化是：
 * 渲染宿主来自 `context.root`（由 bootstrap 决定），而不是写死 `#app`。
 * 渲染树、Provider 顺序、单例 QueryClient、StrictMode、styled-components 的
 * `shouldForwardProp` 过滤器都与迁移前完全一致 —— UI/UX 不变。
 */

// 必须排在所有其它 import 之前：先装好 Node 全局兜底，再求值任何依赖。
import "./node-shim.ts";

import { BaseStyles, ThemeProvider } from "@primer/react";
import { StrictMode } from "react";
import isPropValid from "@emotion/is-prop-valid";
import { StyleSheetManager } from "styled-components";
import ReactDOM from "react-dom/client";
import type { FloatCTFFrontendModule, FloatCTFMountContext } from "@floatctf/frontend-runtime";

import * as TanStackQueryProvider from "./integrations/tanstack-query/root-provider.tsx";
import {
	RouterProvider,
	TanStackQueryProviderContext,
	router,
} from "./router.tsx";
import reportWebVitals from "./reportWebVitals.ts";
// 样式：dev 由 Vite 注入；生产构建会抽成独立 CSS，由 bootstrap 按 frontend.json 注入。
import "./style.css";

let mountedRoot: HTMLElement | null = null;

export function mount(context: FloatCTFMountContext): void {
	if (mountedRoot === context.root) return; // 幂等：同一宿主重复 mount 直接忽略
	mountedRoot = context.root;

	const root = ReactDOM.createRoot(context.root);
	root.render(
		<StrictMode>
			<StyleSheetManager
				shouldForwardProp={(propName, target) =>
					typeof target !== "string" || isPropValid(propName)
				}
			>
				<TanStackQueryProvider.Provider {...TanStackQueryProviderContext}>
					<ThemeProvider>
						<BaseStyles>
							<RouterProvider router={router} />
						</BaseStyles>
					</ThemeProvider>
				</TanStackQueryProvider.Provider>
			</StyleSheetManager>
		</StrictMode>,
	);

	// 若要测量应用性能，可传入函数（记录结果或发送到分析端点）。
	reportWebVitals();
}

/** 制品自述（诊断用；平台以本地注册表为准）。 */
export const manifest = { id: "default", version: "1.0.0" };

const frontendModule: FloatCTFFrontendModule = { manifest, mount };
export default frontendModule;
