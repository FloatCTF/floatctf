import { AppLink } from "@/navigation";
import {
	type AdminNavigationNode,
	EMPTY_NAVIGATION_VISIBILITY,
	type NavigationGroup,
	type NavigationSection,
	type NavigationVisibilityContext,
} from "@/navigation/sidebar-types";
import {
	defaultExpandedGroupIds,
	filterNavigationSections,
	findActiveNavigationPath,
	groupDomId,
	parseStoredExpandedIds,
	serializeExpandedIds,
} from "@/navigation/sidebar-utils";
import { ChevronRightIcon } from "@primer/octicons-react";
import { useLocation } from "@tanstack/react-router";
import { Fragment, useCallback, useEffect, useMemo, useState } from "react";

const DEFAULT_STORAGE_KEY = "floatctf.admin.navigation.expanded";

export interface HierarchicalSideBarProps
	extends React.HTMLAttributes<HTMLElement> {
	sections: readonly NavigationSection[];
	storageKey?: string;
	visibility?: NavigationVisibilityContext;
	ariaLabel?: string;
}

function readStoredExpanded(storageKey: string): Set<string> {
	try {
		if (typeof window === "undefined" || !window.localStorage) return new Set();
		return parseStoredExpandedIds(window.localStorage.getItem(storageKey));
	} catch {
		return new Set();
	}
}

function writeStoredExpanded(
	storageKey: string,
	ids: ReadonlySet<string>,
): void {
	try {
		if (typeof window === "undefined" || !window.localStorage) return;
		window.localStorage.setItem(storageKey, serializeExpandedIds(ids));
	} catch {
		// 私密模式等 localStorage 不可写时静默降级，导航不失效
	}
}

function loadExpandedGroups(
	storageKey: string,
	defaults: ReadonlySet<string>,
): Set<string> {
	const expanded = new Set(defaults);
	for (const id of readStoredExpanded(storageKey)) expanded.add(id);
	return expanded;
}

export function HierarchicalSideBar({
	sections,
	storageKey = DEFAULT_STORAGE_KEY,
	visibility = EMPTY_NAVIGATION_VISIBILITY,
	ariaLabel = "Admin navigation",
	className,
	...props
}: HierarchicalSideBarProps) {
	const pathname = useLocation({ select: (location) => location.pathname });
	const visibleSections = useMemo(
		() => filterNavigationSections(sections, visibility),
		[sections, visibility],
	);
	const activePath = useMemo(
		() => findActiveNavigationPath(visibleSections, pathname),
		[visibleSections, pathname],
	);
	const activeAncestors = useMemo(
		() => new Set(activePath.ancestorNodeIds),
		[activePath.ancestorNodeIds],
	);
	const defaultExpanded = useMemo(
		() => defaultExpandedGroupIds(visibleSections),
		[visibleSections],
	);
	const [userExpanded, setUserExpanded] = useState<Set<string>>(() =>
		loadExpandedGroups(storageKey, defaultExpanded),
	);
	const [manualCollapsed, setManualCollapsed] = useState<Set<string>>(
		() => new Set(),
	);

	// 手动折叠仅作用于当前 location。新的子孙路由或
	// 刷新会再次展开路由要求的祖先节点。
	useEffect(() => {
		setManualCollapsed(new Set());
	}, [pathname]);

	useEffect(() => {
		writeStoredExpanded(storageKey, userExpanded);
	}, [storageKey, userExpanded]);

	const isExpanded = useCallback(
		(groupId: string) =>
			!manualCollapsed.has(groupId) &&
			(userExpanded.has(groupId) ||
				defaultExpanded.has(groupId) ||
				activeAncestors.has(groupId)),
		[activeAncestors, defaultExpanded, manualCollapsed, userExpanded],
	);

	const toggleGroup = useCallback(
		(group: NavigationGroup) => {
			const expanded = isExpanded(group.id);
			setUserExpanded((current) => {
				const next = new Set(current);
				if (expanded) next.delete(group.id);
				else next.add(group.id);
				return next;
			});
			setManualCollapsed((current) => {
				const next = new Set(current);
				if (expanded) next.add(group.id);
				else next.delete(group.id);
				return next;
			});
		},
		[isExpanded],
	);

	const renderNode = (node: AdminNavigationNode, depth: number) => {
		if (node.type === "separator") {
			return <li key={node.id} className="floatctf-sidebar-separator" />;
		}

		const rowStyle = { "--sidebar-depth": depth } as React.CSSProperties;
		if (node.type === "item") {
			const active = activePath.activeNodeId === node.id;
			const rowClass = `floatctf-sidebar-row floatctf-sidebar-link${active ? " is-active" : ""}${node.disabled ? " is-disabled" : ""}`;
			const visual = (
				<>
					{depth === 0 && node.icon && (
						<span className="floatctf-sidebar-icon" aria-hidden="true">
							{node.icon}
						</span>
					)}
					<span className="floatctf-sidebar-label" title={node.label}>
						{node.label}
					</span>
					{node.badge && (
						<span className="floatctf-sidebar-badge">{node.badge}</span>
					)}
				</>
			);
			return (
				<li key={node.id} className="floatctf-sidebar-list-item">
					{node.disabled ? (
						<span className={rowClass} style={rowStyle} aria-disabled="true">
							{visual}
						</span>
					) : (
						<AppLink
							to={node.href}
							preload="intent"
							aria-current={active ? "page" : undefined}
							className={rowClass}
							style={rowStyle}
						>
							{visual}
						</AppLink>
					)}
				</li>
			);
		}

		const expanded = isExpanded(node.id);
		const containsActive = activeAncestors.has(node.id);
		const childrenId = groupDomId(node.id);
		return (
			<li key={node.id} className="floatctf-sidebar-list-item">
				<button
					type="button"
					className={`floatctf-sidebar-row floatctf-sidebar-group${containsActive ? " contains-active" : ""}`}
					style={rowStyle}
					aria-expanded={expanded}
					aria-controls={childrenId}
					onClick={() => toggleGroup(node)}
				>
					{depth === 0 && node.icon && (
						<span className="floatctf-sidebar-icon" aria-hidden="true">
							{node.icon}
						</span>
					)}
					<span className="floatctf-sidebar-label" title={node.label}>
						{node.label}
					</span>
					<ChevronRightIcon
						className={`floatctf-sidebar-chevron${expanded ? " is-expanded" : ""}`}
						aria-hidden="true"
					/>
				</button>
				{expanded && (
					<ul id={childrenId} className="floatctf-sidebar-list">
						{node.children.map((child) => renderNode(child, depth + 1))}
					</ul>
				)}
			</li>
		);
	};

	return (
		<nav
			aria-label={ariaLabel}
			className={`floatctf-sidebar${className ? ` ${className}` : ""}`}
			{...props}
		>
			{visibleSections.map((section, index) => (
				<Fragment key={section.id}>
					<section
						className={`floatctf-sidebar-section${index === 0 ? " is-first" : ""}`}
						aria-labelledby={
							section.label ? `admin-nav-section-${section.id}` : undefined
						}
					>
						{section.label && (
							<h2
								id={`admin-nav-section-${section.id}`}
								className="floatctf-sidebar-section-label"
							>
								{section.label}
							</h2>
						)}
						<ul className="floatctf-sidebar-list">
							{section.children.map((node) => renderNode(node, 0))}
						</ul>
					</section>
				</Fragment>
			))}
		</nav>
	);
}
