import { defineConfig } from "vitest/config";

export default defineConfig({
	test: {
		environment: "jsdom",
		include: ["src/**/*.test.ts"],
		// 契约/校验测试是纯逻辑，不需要网络与真实浏览器。
		passWithNoTests: false,
	},
});
