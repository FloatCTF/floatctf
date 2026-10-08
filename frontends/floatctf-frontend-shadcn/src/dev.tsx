/**
 * dev 入口 —— 直接把本前端挂到 `#root`（与生产制品调用**同一个** `mount(context)`）。
 *
 * 生产走：apps/web bootstrap → GET /api/frontend → 本地注册表 → 版本化 ESM 制品 → mount(context)。
 * 开发走：本文件 → mount(context)。两条路径共用同一 mount，因此契约不会分叉。
 *
 * 这里是唯一允许在 dev 下读取 `/api/frontend` 的地方（仅为拿到真实的 capabilities /
 * platform_version 做本地诊断）；制品本身不依赖该请求。
 */

import type { FloatCTFMountContext } from "@floatctf/frontend-runtime";
import {
	API_CONTRACT_VERSION,
	FRONTEND_RUNTIME_VERSION,
} from "@floatctf/frontend-runtime";

import { mount } from "./entry";

const FALLBACK_CAPABILITIES = [
	"jeopardy",
	"awd",
	"awdp",
	"discussions",
	"writeups",
	"web_terminal",
];

interface BootstrapInfoLike {
	platform_version?: string;
	api_contract_version?: string;
	frontend_runtime_version?: string;
	capabilities?: string[];
}

async function readPlatformInfo(): Promise<BootstrapInfoLike | null> {
	try {
		const response = await fetch("/api/frontend", { headers: { Accept: "application/json" } });
		if (!response.ok) return null;
		const payload = (await response.json()) as { data?: BootstrapInfoLike };
		return payload.data ?? null;
	} catch {
		return null;
	}
}

async function start(): Promise<void> {
	const rootElement = document.getElementById("root");
	if (!rootElement) throw new Error("dev 入口缺少 #root 容器");

	const info = await readPlatformInfo();

	const context: FloatCTFMountContext = {
		root: rootElement,
		apiBaseUrl: "/api",
		// dev 下资产由 Vite 直接服务，站点根即前端根。
		assetBaseUrl: "/",
		frontendId: "shadcn",
		frontendVersion: "0.1.0-dev",
		platformVersion: info?.platform_version ?? "dev",
		apiContractVersion: info?.api_contract_version ?? API_CONTRACT_VERSION,
		frontendRuntimeVersion: info?.frontend_runtime_version ?? FRONTEND_RUNTIME_VERSION,
		capabilities: info?.capabilities ?? FALLBACK_CAPABILITIES,
	};

	const unmount = mount(context);
	if (import.meta.hot) {
		import.meta.hot.dispose(() => unmount());
	}
}

void start();
