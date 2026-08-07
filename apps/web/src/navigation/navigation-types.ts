/**
 * Navigation coordinator phases:
 * - idle: no active navigation
 * - preloading: preloadRoute is in progress
 * - committing: preload complete, navigating
 */
export type NavigationPhase = "idle" | "preloading" | "committing";

/**
 * Loose navigation options for the coordinator.
 * Type safety is enforced at call sites (AppLink props, useAppNavigate return).
 * The coordinator casts to the router's typed NavigateOptions internally.
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
