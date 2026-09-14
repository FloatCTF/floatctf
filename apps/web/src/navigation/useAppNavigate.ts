import { useCallback } from "react";
import { useNavigation } from "./NavigationContext";
import type { CoordinatorNavigateOptions } from "./navigation-types";

/**
 * 命令式 SPA 导航 Hook。
 *
 * 经导航协调器（预加载 → 提交）。
 * 用于用户操作触发的编程式导航
 * （例如侧栏点击、Header 首页按钮）。
 *
 * 声明式链接请用 <AppLink>。
 *
 * @example
 * const appNavigate = useAppNavigate();
 * appNavigate({ to: "/service/top" });
 */
export function useAppNavigate() {
	const { navigateWithTransition } = useNavigation();
	return useCallback(
		(opts: CoordinatorNavigateOptions) => navigateWithTransition(opts),
		[navigateWithTransition],
	);
}
