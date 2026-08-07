import { useCallback } from "react";
import { useNavigation } from "./NavigationContext";
import type { CoordinatorNavigateOptions } from "./navigation-types";

/**
 * Imperative SPA navigation hook.
 *
 * Routes through the Navigation Coordinator (preload → commit).
 * Use for programmatic navigation triggered by user actions
 * (e.g. SideBar click, Header home button).
 *
 * For declarative links, use <AppLink> instead.
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
