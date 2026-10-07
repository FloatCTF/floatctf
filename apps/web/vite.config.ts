import { defineConfig } from "vitest/config";

/**
 * Bootstrap 的构建配置**刻意保持最小**：
 * 没有 React 插件、没有 Tailwind、没有路由插件、没有代码分割魔法。
 * 产物只有 `index.html` + 一个入口 JS —— 任何"bootstrap 悄悄带上默认前端"
 * 的回归都会在体积和 `scripts/check-architecture.sh` 的产物断言上暴露。
 */
export default defineConfig({
	build: {
		outDir: "dist",
		emptyOutDir: true,
		target: "esnext",
		rollupOptions: {
			output: {
				entryFileNames: "assets/bootstrap-[hash].js",
				chunkFileNames: "assets/[name]-[hash].js",
				assetFileNames: "assets/[name]-[hash][extname]",
			},
		},
	},
	test: {
		environment: "node",
	},
});
