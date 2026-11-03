/**
 * 运行时上下文。
 *
 * 存在的理由：SDK 客户端必须在 `mount(context)` 里按 `context.apiBaseUrl` 创建，
 * 而页面在 React 树深处 —— 用 context 传递比模块级单例更安全：
 * 模块级单例会让「同一页面挂载两个实例」互相覆盖（SDK 早期版本正是踩过这个坑）。
 */
import { createContext, useContext } from "react";

import type { Runtime } from "~/api/client";

export const RuntimeContext = createContext<Runtime | null>(null);

export function useRuntime(): Runtime {
	const runtime = useContext(RuntimeContext);
	if (!runtime) {
		throw new Error("useRuntime 必须在 RuntimeContext.Provider 内使用");
	}
	return runtime;
}
