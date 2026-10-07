/**
 * 自包含浏览器制品的 Node 全局兜底。
 *
 * 为什么需要：第三方依赖（styled-components / mermaid 等）里可能存在只在 Node 下
 * 才会走的 `process.*` 引用。构建期已用 Vite `define` 替换了 `process.env.*`，
 * 这里再兜一层 `globalThis.process`，防止漏网的引用把整页打挂。
 *
 * 约束（见 docs/frontend/ARTIFACT.md）：前端制品运行在**纯浏览器**环境里 ——
 * 不要依赖 Node 全局、不要依赖 `require`、不要依赖打包器注入的 `__dirname`。
 */
// 刻意用 `Record<string, unknown>`：这里**不是**在实现 Node 的 `Process` 类型
// （@types/node 在本包可见，直接赋值会撞上完整 Process 结构），
// 只是给浏览器里漏网的 `process.*` 引用一个不会抛错的落点。
const g = globalThis as unknown as { process?: Record<string, unknown> };

if (typeof g.process === "undefined") {
	g.process = {
		env: { NODE_ENV: "production" },
		platform: "browser",
		version: "",
		nextTick: (callback: (...args: unknown[]) => void, ...args: unknown[]) => {
			queueMicrotask(() => callback(...args));
		},
		emit: () => false,
		on: () => undefined,
		once: () => undefined,
		off: () => undefined,
		removeListener: () => undefined,
		cwd: () => "/",
	};
}
