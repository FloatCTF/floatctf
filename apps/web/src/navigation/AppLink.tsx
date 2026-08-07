import { Link } from "@tanstack/react-router";
import { useNavigation } from "./NavigationContext";
import type { CoordinatorNavigateOptions } from "./navigation-types";

/**
 * Type-safe SPA link wrapper around TanStack Router's Link.
 *
 * Intercepts plain left-clicks and routes them through the Navigation
 * Coordinator (preload → commit). The following bypass the coordinator
 * and use normal browser behavior:
 * - modifier clicks (Ctrl/Cmd/Shift/Alt)
 * - non-left clicks (middle/right)
 * - target != "_self"
 * - download attribute
 * - external `to` (contains ":" e.g. https://, mailto:, tel:)
 * - reloadDocument
 *
 * Preserves:
 * - TanStack Router typed navigation (to/params/search/hash)
 * - defaultPreload="intent" behavior (hover/touch intent preload)
 * - Link active state (activeProps/inactiveProps)
 */
export interface AppLinkProps {
	/** Target route path. External URLs (https://…, mailto:…) bypass SPA. */
	to: string;
	params?: Record<string, any>;
	search?: Record<string, any>;
	hash?: string;
	preload?: false | "intent" | "viewport" | "render";
	/** Link target. Any value other than "_self" bypasses the coordinator. */
	target?: string;
	/** Download link — rendered as a plain anchor, bypasses coordinator. */
	download?: string | boolean;
	/** External anchor href — rendered as a plain anchor, bypasses coordinator. */
	href?: string;
	/** Force full page reload — bypasses coordinator. */
	reloadDocument?: boolean;
	className?: string;
	style?: React.CSSProperties;
	"aria-current"?: React.AriaAttributes["aria-current"];
	onClick?: React.MouseEventHandler<HTMLAnchorElement>;
	children?: React.ReactNode;
}

/** Determine whether this click should bypass the coordinator. */
function shouldBypass(
	event: React.MouseEvent<HTMLAnchorElement>,
	props: AppLinkProps,
): boolean {
	if (event.button !== 0) return true; // non-left click
	if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey)
		return true;
	if (event.defaultPrevented) return true;
	if (props.target && props.target !== "_self") return true;
	if (props.download !== undefined) return true;
	if (props.reloadDocument) return true;
	// External URL / mailto: / tel: → TanStack Link treats these as external
	if (typeof props.to === "string" && props.to.includes(":")) return true;
	return false;
}

export function AppLink({
	onClick,
	children,
	target,
	download,
	href,
	reloadDocument,
	style,
	className,
	"aria-current": ariaCurrent,
	preload,
	...navigateOpts
}: AppLinkProps) {
	const { navigateWithTransition } = useNavigation();

	// download / explicit external href → plain anchor, never SPA navigation
	if (download !== undefined || href !== undefined) {
		return (
			<a
				href={href ?? navigateOpts.to}
				download={
					typeof download === "string" ? download : download ? "" : undefined
				}
				target={target}
				style={style}
				className={className}
				onClick={onClick}
			>
				{children}
			</a>
		);
	}

	const handleClick = (event: React.MouseEvent<HTMLAnchorElement>) => {
		if (shouldBypass(event, { ...navigateOpts, target, reloadDocument })) {
			onClick?.(event);
			return;
		}

		// Intercept: prevent default browser navigation
		event.preventDefault();
		onClick?.(event);

		const navOpts: CoordinatorNavigateOptions = {
			to: navigateOpts.to,
			params: navigateOpts.params,
			search: navigateOpts.search,
			hash: navigateOpts.hash,
			preload,
			target,
			reloadDocument,
		};

		void navigateWithTransition(navOpts);
	};

	return (
		<Link
			to={navigateOpts.to}
			params={navigateOpts.params as never}
			search={navigateOpts.search as never}
			hash={navigateOpts.hash}
			target={target}
			reloadDocument={reloadDocument}
			preload={preload}
			style={style}
			className={className}
			aria-current={ariaCurrent}
			onClick={handleClick}
		>
			{children}
		</Link>
	);
}
