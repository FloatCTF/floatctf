import { useNavigation } from "@/navigation";
import { service_routes } from "@/routes";
import { NavList } from "@primer/react";
import { useLocation, useRouter } from "@tanstack/react-router";
import { useCallback } from "react";

export type NavRoute = {
	label: string;
	path?: string;
	icon: React.ReactNode;
	children?: NavRoute[];
};
export interface GenericSideBarProps
	extends React.HTMLAttributes<HTMLDivElement> {
	routes: NavRoute[];
}

export const GenericSideBar = ({ routes, ...props }: GenericSideBarProps) => {
	const location = useLocation();
	const router = useRouter();
	const { navigateWithTransition } = useNavigation();

	// Intent preload: on mouse enter / focus, preload the target route's chunk
	// so that the subsequent click is near-instant.
	const handleIntentPreload = useCallback(
		(path: string | undefined) => () => {
			if (!path) return;
			router.preloadRoute({ to: path }).catch(() => {});
		},
		[router],
	);

	// SPA navigation via coordinator: intercept plain left-click,
	// start preload → commit. Modifier clicks use browser default.
	const handleNav = useCallback(
		(path: string | undefined) => (event: React.MouseEvent<HTMLElement>) => {
			if (!path) return;
			if (
				event.defaultPrevented ||
				event.button !== 0 ||
				event.metaKey ||
				event.ctrlKey ||
				event.shiftKey ||
				event.altKey
			)
				return;
			event.preventDefault();
			void navigateWithTransition({ to: path });
		},
		[navigateWithTransition],
	);

	return (
		<NavList {...props}>
			{routes.map((route, index) => (
				<NavList.Item
					key={`${route.path}-${index}`}
					href={route.path}
					onClick={handleNav(route.path)}
					onMouseEnter={handleIntentPreload(route.path)}
					onFocus={handleIntentPreload(route.path)}
					defaultOpen={route.children?.some(
						(c) => c.path === location.pathname,
					)}
					aria-current={
						location.pathname.startsWith(route.path ?? "") ? "page" : undefined
					}
				>
					<NavList.LeadingVisual>{route.icon}</NavList.LeadingVisual>
					{route.label}

					{route.children && (
						<NavList.SubNav>
							{route.children.map((child) => (
								<NavList.Item
									key={child.path}
									href={child.path}
									onClick={handleNav(child.path)}
									onMouseEnter={handleIntentPreload(child.path)}
									onFocus={handleIntentPreload(child.path)}
									aria-current={
										location.pathname === child.path ? "page" : undefined
									}
								>
									<NavList.LeadingVisual>{child.icon}</NavList.LeadingVisual>
									{child.label}
								</NavList.Item>
							))}
						</NavList.SubNav>
					)}
				</NavList.Item>
			))}
		</NavList>
	);
};
