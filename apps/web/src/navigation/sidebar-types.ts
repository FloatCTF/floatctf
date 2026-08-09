import type { ReactNode } from "react";

export type NavigationMatchRule =
	| { mode: "exact"; path?: string }
	| { mode: "segment-prefix"; path?: string }
	| { mode: "pattern"; pattern: RegExp };

export interface NavigationVisibilityContext {
	permissions: ReadonlySet<string>;
	features: ReadonlySet<string>;
}

export type NavigationVisibility = (
	context: NavigationVisibilityContext,
) => boolean;

interface NavigationNodeBase {
	id: string;
	label: string;
	icon?: ReactNode;
	isVisible?: NavigationVisibility;
}

export interface NavigationItem extends NavigationNodeBase {
	type: "item";
	href: string;
	match: NavigationMatchRule;
	badge?: ReactNode;
	disabled?: boolean;
}

export interface NavigationGroup extends NavigationNodeBase {
	type: "group";
	children: AdminNavigationNode[];
	defaultExpanded?: boolean;
}

export interface NavigationSeparator {
	type: "separator";
	id: string;
	isVisible?: NavigationVisibility;
}

export type AdminNavigationNode =
	| NavigationItem
	| NavigationGroup
	| NavigationSeparator;

export interface NavigationSection {
	id: string;
	label?: string;
	children: AdminNavigationNode[];
	isVisible?: NavigationVisibility;
}

export interface ActiveNavigationPath {
	activeNodeId: string | null;
	ancestorNodeIds: string[];
}

export const EMPTY_NAVIGATION_VISIBILITY: NavigationVisibilityContext = {
	permissions: new Set<string>(),
	features: new Set<string>(),
};
