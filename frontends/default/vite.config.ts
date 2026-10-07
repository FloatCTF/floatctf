import { defineConfig, type Plugin } from "vitest/config";
import viteReact from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { TanStackRouterVite } from "@tanstack/router-plugin/vite";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import pkg from "./package.json";

const FRONTEND_ID = "default";
const FRONTEND_NAME = "FloatCTF Default Frontend";
/** 与 packages/frontend-runtime/src/version.ts 保持一致（契约 major）。 */
const FRONTEND_RUNTIME_CONTRACT = "1";
const API_CONTRACT = "1";
/** 生产制品入口固定文件名：注册表与 Caddy 缓存策略都依赖它的确定性。 */
const ENTRY_FILE = "assets/frontend.js";
const STYLE_FILE = "assets/frontend.css";

function manifestObject(styles: string[]) {
	return {
		schemaVersion: 1,
		id: FRONTEND_ID,
		name: FRONTEND_NAME,
		version: pkg.version,
		description: "FloatCTF 官方前端（当前完整 UI）",
		author: "FloatCTF",
		compatibility: {
			frontendRuntime: FRONTEND_RUNTIME_CONTRACT,
			apiContract: API_CONTRACT,
		},
		entry: ENTRY_FILE,
		styles,
	};
}

/**
 * 构建结束后写出 `frontend.json`，并用 `@floatctf/frontend-runtime` 的**真实校验器**
 * 校验它。这样"能构建但装不上"（路径写成绝对路径、契约版本写错、ID 非法）在构建期
 * 就会失败，而不是等运维在服务器上安装时才发现。
 */
function emitFrontendManifest(): Plugin {
	return {
		name: "floatctf:emit-frontend-manifest",
		apply: "build",
		async closeBundle() {
			const outDir = resolve(__dirname, "dist");
			const styles = existsSync(resolve(outDir, STYLE_FILE)) ? [STYLE_FILE] : [];
			const manifest = manifestObject(styles);
			const { parseFrontendManifest } = await import("@floatctf/frontend-runtime");
			const parsed = parseFrontendManifest(manifest);
			if (!parsed.ok) {
				throw new Error(
					`generated frontend.json does not satisfy the frontend runtime contract:\n${parsed.errors.join("\n")}`,
				);
			}
			if (!existsSync(resolve(outDir, ENTRY_FILE))) {
				throw new Error(`frontend entry was not emitted: ${ENTRY_FILE}`);
			}
			mkdirSync(outDir, { recursive: true });
			writeFileSync(
				resolve(outDir, "frontend.json"),
				`${JSON.stringify(manifest, null, 2)}\n`,
			);
		},
	};
}

/**
 * DEV ONLY：让开发服务器同时提供"本地已安装前端注册表"。
 *
 * 生产环境这份文件由 Caddy 从 `$FLOATCTF_HOME/frontends/registry.json` 提供。
 * 开发环境没有安装目录，但管理端的"前端选择器"要真实读取注册表，因此这里
 * 按注册表的**真实 schema** 生成一份只含 default 的开发注册表 —— 选择器在 dev
 * 下走的是与生产完全相同的代码路径与数据格式。
 */
function devRegistry(): Plugin {
	const registry = {
		schemaVersion: 1,
		updatedAt: new Date(0).toISOString(),
		frontends: {
			[FRONTEND_ID]: {
				id: FRONTEND_ID,
				currentVersion: pkg.version,
				protected: true,
				versions: {
					[pkg.version]: {
						version: pkg.version,
						name: FRONTEND_NAME,
						description: "FloatCTF 官方前端（当前完整 UI）",
						author: "FloatCTF",
						compatibility: {
							frontendRuntime: FRONTEND_RUNTIME_CONTRACT,
							apiContract: API_CONTRACT,
						},
						entry: ENTRY_FILE,
						styles: [STYLE_FILE],
						installedAt: new Date(0).toISOString(),
					},
				},
			},
		},
	};
	return {
		name: "floatctf:dev-registry",
		apply: "serve",
		configureServer(server) {
			server.middlewares.use((req, res, next) => {
				if (!req.url?.startsWith("/__floatctf/frontends/registry.json")) {
					next();
					return;
				}
				res.setHeader("Content-Type", "application/json");
				res.setHeader("Cache-Control", "no-store");
				res.end(JSON.stringify(registry));
			});
		},
	};
}

// 单元测试（vitest）跑在**开发** React 下：`React.act` 只在 development 构建里导出。
// 因此 `process.env.*` 的替换**只在构建时**生效，绝不污染测试环境
// （否则 React 会被解析成 production 构建，act/测试工具全挂）。
const isTest = Boolean(process.env.VITEST);

export default defineConfig({
	// 生产制品由 bootstrap 从 /__floatctf/frontends/<id>/<version>/ 动态 import，
	// 因此所有内部引用必须是相对路径（绝不写死站点根）。
	base: "./",
	define: {
		"import.meta.env.VITE_APP_VERSION": JSON.stringify(pkg.version),
		// 关键：生产制品是一个**自包含的浏览器应用**，不是给别的打包器消费的库。
		// Vite 的 lib 模式刻意**不**替换第三方依赖里的 `process.env.*`
		// （默认假设消费者会处理），而浏览器里根本没有 `process` ——
		// 一旦引用执行就是 "process is not defined"（实测：默认前端整页起不来，
		// 由 bootstrap 的兜底页如实报错）。因此构建时必须显式替换。
		...(isTest
			? {}
			: {
					"process.env.NODE_ENV": JSON.stringify("production"),
					"process.env": JSON.stringify({ NODE_ENV: "production" }),
				}),
	},
	plugins: [
		// Vitest 下关闭路由代码分割：分割后的页面是懒加载组件，
		// 单测里渲染路由页面会一直停在 Suspense fallback。开发/构建仍开启。
		TanStackRouterVite({ autoCodeSplitting: !isTest }),
		viteReact(),
		tailwindcss(),
		devRegistry(),
		emitFrontendManifest(),
	],
	resolve: {
		alias: {
			"@": resolve(__dirname, "./src"),
		},
	},
	server: {
		// host: true 让 dev server 监听 0.0.0.0，
		// 这样 Caddy 容器可通过 host-gateway (172.17.0.1:13000) 反向代理。
		host: true,
		watch: {
			ignored: ["**/routeTree.gen.ts"],
		},
		// 启动时预编译常用页面，避免开发时首次点击标签页要等编译。
		warmup: {
			clientFiles: [
				"./src/routes/service/index.tsx",
				"./src/routes/service/top.tsx",
				"./src/routes/service/challenges/index.tsx",
				"./src/routes/admin/index.tsx",
				"./src/routes/admin/dashboard.tsx",
				"./src/routes/admin/challenges.tsx",
			],
		},
	},
	build: {
		outDir: "dist",
		emptyOutDir: true,
		target: "esnext",
		// 单文件 CSS：bootstrap 需要按 frontend.json 显式注入，文件名必须确定。
		cssCodeSplit: false,
		// 前端自带框架与依赖（v1 不做共享 React 单例 / 模块联邦）。
		lib: {
			entry: resolve(__dirname, "src/entry.tsx"),
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
						: "assets/[name][extname]",
			},
		},
	},
	test: {
		// 与迁移前的 apps/web 一致：默认 Node 环境，需要 DOM 的用例在文件首行
		// 加 `// @vitest-environment jsdom`。
		environment: "node",
	},
});

/** package.json 的版本同时用于 manifest 与注册表，避免两处漂移。 */
export const frontendPackageVersion = pkg.version;
export const frontendEntryFile = ENTRY_FILE;
export const frontendStyleFile = STYLE_FILE;

// 让 IDE/测试能引用到读取到的 manifest（构建期由 emitFrontendManifest 校验）。
export function readBuiltManifest(distDir = resolve(__dirname, "dist")) {
	const raw = readFileSync(resolve(distDir, "frontend.json"), "utf8");
	return JSON.parse(raw) as Record<string, unknown>;
}
