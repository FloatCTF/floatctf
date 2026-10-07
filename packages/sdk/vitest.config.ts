import { defineConfig } from "vitest/config";

export default defineConfig({
	test: {
		// 默认 Node 环境：SDK 是框架/浏览器无关的传输与协议代码。
		// 需要 DOM 的用例在文件首行加 `// @vitest-environment jsdom`。
		environment: "node",
		include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
		passWithNoTests: false,
	},
});
