import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import viteReact from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

import sourceManifest from "./floatctf.frontend.json" with { type: "json" };

const ENTRY_FILE = "assets/frontend.js";
const STYLE_FILE = "assets/frontend.css";

/**
 * 构建结束写 `frontend.json`，并用**真实校验器**自检 —— 「能构建但装不上」在构建期即失败。
 *
 * 注意制品 manifest **不得**包含 `build` 段（严格校验会拒绝未知字段），
 * 因此这里逐字段重建，而不是直接展开 `sourceManifest`。
 */
function emitFrontendManifest(): Plugin {
	return {
		name: "floatctf:emit-frontend-manifest",
		apply: "build",
		async closeBundle() {
			const outDir = resolve(import.meta.dirname, "dist");
			const styles = existsSync(resolve(outDir, STYLE_FILE)) ? [STYLE_FILE] : [];
			const manifest = {
				schemaVersion: sourceManifest.schemaVersion,
				id: sourceManifest.id,
				name: sourceManifest.name,
				version: sourceManifest.version,
				description: sourceManifest.description,
				author: sourceManifest.author,
				compatibility: sourceManifest.compatibility,
				entry: ENTRY_FILE,
				styles,
			};
			const { parseFrontendManifest } = await import("@floatctf/frontend-runtime");
			const parsed = parseFrontendManifest(manifest);
			if (!parsed.ok) {
				throw new Error(
					`生成的 frontend.json 不满足前端运行时契约：\n${parsed.errors.join("\n")}`,
				);
			}
			if (!existsSync(resolve(outDir, ENTRY_FILE))) {
				throw new Error(`未产出入口文件：${ENTRY_FILE}`);
			}
			mkdirSync(outDir, { recursive: true });
			writeFileSync(
				resolve(outDir, "frontend.json"),
				`${JSON.stringify(manifest, null, 2)}\n`,
			);
		},
	};
}

export default defineConfig({
	// 生产制品由 bootstrap 从 /__floatctf/frontends/<id>/<version>/ 动态 import，
	// 因此所有内部引用必须是相对路径（绝不写死站点根）。
	base: "./",
	define: {
		// 制品是自包含浏览器应用：浏览器里没有 process。
		"process.env.NODE_ENV": JSON.stringify("production"),
		"process.env": JSON.stringify({ NODE_ENV: "production" }),
	},
	plugins: [viteReact(), emitFrontendManifest()],
	resolve: {
		// `~/...` 是本前端自己的内部 alias（tsconfig `paths` 的同名映射）。
		// 它是**本包内部**的约定，不是 monorepo 的 `@/` 私有 alias。
		alias: [{ find: /^~\//, replacement: `${resolve(import.meta.dirname, "src")}/` }],
	},
	server: {
		host: true,
		// 开发时同源代理到 FloatCTF 开发入口（Caddy :7780），apiBaseUrl 仍是 "/api"，
		// 与生产同源语义一致，不需要改后端 CORS 配置。
		proxy: {
			// ws: true 是必需的：管理端 Web 终端要经 /api/admin/terminal/ws 升级协议。
			"/api": { target: "http://127.0.0.1:7780", changeOrigin: true, ws: true },
			"/public": { target: "http://127.0.0.1:7780", changeOrigin: true },
			"/private": { target: "http://127.0.0.1:7780", changeOrigin: true },
			"/static": { target: "http://127.0.0.1:7780", changeOrigin: true },
		},
	},
	build: {
		outDir: "dist",
		emptyOutDir: true,
		target: "esnext",
		// 已知取舍：库自带 9 个 woff2（Nunito + Noto Sans SC，其中 CJK 子集约 3.8MB），
		// 其 CSS 用相对 `url("files/...")` 引用它们，Vite 会把它们 **base64 内联**进
		// 单个 CSS —— 实测 `assetsInlineLimit: 0` 对 CSS 里的字体引用**不生效**
		// （已实测：设置后产物仍是 4.88MB、dist/assets 下没有 woff2），
		// 因此这里不写那条无效配置，避免留下会误导后来人的死设置。
		//
		// 结果是产物**完全自包含**（不会出现字体 404），代价是首屏 CSS 较大
		// （4.88MB / gzip 3.60MB）。若要优化：删掉 CSS 里 noto-sans-sc 的
		// `@font-face` 块即可 —— 字体栈里已有 `PingFang SC / Microsoft YaHei`
		// 兜底，中文会改用系统字体，CSS 会回落到 ~126KB。
		// 全站只有一个 CSS 文件：bootstrap 按 frontend.json 的 styles 注入，
		// 拆成多份会让「样式是否加载」变成一个需要额外编排的问题。
		cssCodeSplit: false,
		lib: {
			entry: resolve(import.meta.dirname, "src/entry.tsx"),
			formats: ["es"],
			fileName: () => ENTRY_FILE,
		},
		rollupOptions: {
			output: {
				entryFileNames: ENTRY_FILE,
				chunkFileNames: "assets/[name]-[hash].js",
				assetFileNames: (assetInfo) =>
					assetInfo.names?.some((name) => name.endsWith(".css"))
						? STYLE_FILE
						: "assets/[name]-[hash][extname]",
			},
		},
	},
});
