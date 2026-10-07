/**
 * DEV ONLY 入口（`frontends/default/index.html` 加载它）。
 *
 * 开发模式下 Default Frontend 直接挂载**工作区源码**，保留 Vite HMR；
 * 生产模式走 `apps/web` bootstrap → 本地注册表 → 版本化 ESM 制品。
 * 两条路径都最终调用同一个 `mount(context)`，因此契约不会分叉。
 */

import { DEFAULT_API_BASE_URL } from "@floatctf/frontend-runtime";

import { mount } from "./entry.tsx";
import pkg from "../package.json";

const root = document.getElementById("app");
if (root) {
	mount({
		root,
		apiBaseUrl: DEFAULT_API_BASE_URL,
		assetBaseUrl: "/",
		frontendId: "default",
		frontendVersion: pkg.version,
		platformVersion: "dev",
		apiContractVersion: "1",
		frontendRuntimeVersion: "1",
		capabilities: ["jeopardy", "awd", "awdp", "discussions", "writeups", "web_terminal"],
	});
}
