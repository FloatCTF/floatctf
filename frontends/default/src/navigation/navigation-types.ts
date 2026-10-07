/**
 * 导航协调器阶段：
 * - idle：无进行中的导航
 * - preloading：preloadRoute 进行中
 * - committing：预加载完成，正在导航
 */
export type NavigationPhase = "idle" | "preloading" | "committing";

/**
 * 协调器使用的宽松导航选项。
 * 类型安全在调用点保证（AppLink props、useAppNavigate 返回值）。
 * 协调器内部再转为 router 的类型化 NavigateOptions。
 */
export interface CoordinatorNavigateOptions {
	to: string;
	params?: Record<string, any>;
	search?: Record<string, any>;
	hash?: string;
	preload?: false | "intent" | "viewport" | "render";
	target?: string;
	reloadDocument?: boolean;
}
