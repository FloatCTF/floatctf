/**
 * FloatCTF web bootstrap —— **不是 UI**。
 *
 * 它只做"把已安装的前端加载起来"这件事，全部机制在
 * `@floatctf/frontend-runtime` 的 `bootstrapFrontend()` 里：
 *
 * ```
 * GET /api/frontend            → 平台版本 / API 契约 / FRONTEND_ACTIVE / capabilities
 * GET /__floatctf/frontends/registry.json → 本地已安装前端
 * → 客户端校验兼容性（runtime / API 契约 major）
 * → 注入该前端的样式
 * → 动态 import 同源 ESM（/__floatctf/frontends/<id>/<version>/...）
 * → frontend.mount(context)
 * → 失败回退 default → 再失败渲染内置兜底页
 * ```
 *
 * 刻意保持无框架、无 Primer、无路由、无状态库：bootstrap 越"无聊"，它就越不会
 * 成为前端演进（甚至换成非 React 前端）的瓶颈。
 *
 * 破窗恢复：`?frontend=default`。该参数**只**影响当前这次页面加载，
 * 且只接受本地注册表里**已安装**的安全前端 ID —— 不写后端设置、不需要登录。
 */

import {
	bootstrapFrontend,
	renderBootstrapEmergencyUi,
	type BootstrapDiagnostics,
} from "@floatctf/frontend-runtime";

const ROOT_ID = "app";

function readOverride(): string | null {
	try {
		return new URLSearchParams(window.location.search).get("frontend");
	} catch {
		return null;
	}
}

function mount(): void {
	const root = document.getElementById(ROOT_ID);
	if (!root) {
		// index.html 缺 #app 说明构建产物被篡改/损坏：直接给兜底页而不是白屏。
		renderBootstrapEmergencyUi(document.body, {
			attempted: [],
			errors: [`bootstrap: #${ROOT_ID} element is missing from index.html`],
		});
		return;
	}
	// 清掉可能存在的构建期 loading 占位，保证前端挂到一个干净宿主上。
	root.textContent = "";

	const override = readOverride();

	void bootstrapFrontend({
		root,
		overrideFrontendId: override,
		// 兜底页接管整页（它会先清空宿主）。
		emergencyHost: document.body,
	})
		.then((result) => {
			if (!result.mounted) {
				// bootstrapFrontend 已经渲染了兜底页；这里只在控制台留下诊断，
				// 不额外暴露任何信息到页面上。
				console.error("[floatctf] frontend bootstrap failed", result.diagnostics);
			}
		})
		.catch((error: unknown) => {
			// bootstrapFrontend 契约上不抛错；这一层只是防止"兜底页本身都失败"。
			const diagnostics: BootstrapDiagnostics = {
				attempted: [],
				errors: [error instanceof Error ? error.message : String(error)],
			};
			renderBootstrapEmergencyUi(document.body, diagnostics);
		});
}

if (document.readyState === "loading") {
	document.addEventListener("DOMContentLoaded", mount, { once: true });
} else {
	mount();
}
