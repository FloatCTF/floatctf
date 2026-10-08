import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import tailwindcss from "@tailwindcss/vite";
import viteReact from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

import sourceManifest from "./floatctf.frontend.json" with { type: "json" };

const ENTRY_FILE = "assets/frontend.js";
const STYLE_FILE = "assets/frontend.css";

/**
 * 构建结束写 `frontend.json`，并用**真实校验器**自检 —— 「能构建但装不上」在构建期即失败。
 * 制品 manifest 不得包含 `build` 段（严格校验会拒绝未知字段）。
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
	plugins: [viteReact(), tailwindcss(), emitFrontendManifest()],
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
		cssCodeSplit: false,
		lib: {
			entry: resolve(import.meta.dirname, "src/entry.tsx"),
			formats: ["es"],
			fileName: () => "assets/frontend.js",
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
