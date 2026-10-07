import type {
	ActiveNavigationPath,
	AdminNavigationNode,
	NavigationGroup,
	NavigationItem,
	NavigationMatchRule,
	NavigationSection,
	NavigationVisibilityContext,
} from "./sidebar-types";

function normalizePath(path: string): string {
	if (path.length > 1 && path.endsWith("/")) return path.slice(0, -1);
	return path || "/";
}

function rulePath(item: NavigationItem, rule: NavigationMatchRule): string {
	return normalizePath("path" in rule && rule.path ? rule.path : item.href);
}

/** 段匹配将 `/events` 与 `/events/...` 视为相关，但不匹配 `/events-old`。 */
export function matchNavigationItem(
	item: NavigationItem,
	pathname: string,
): boolean {
	const current = normalizePath(pathname);
	switch (item.match.mode) {
		case "exact":
			return current === rulePath(item, item.match);
		case "segment-prefix": {
			const prefix = rulePath(item, item.match);
			return current === prefix || current.startsWith(`${prefix}/`);
		}
		case "pattern":
			item.match.pattern.lastIndex = 0;
			return item.match.pattern.test(current);
	}
}

function matchScore(item: NavigationItem, pathname: string): number {
	if (!matchNavigationItem(item, pathname)) return -1;
	switch (item.match.mode) {
		case "exact":
			return 3_000_000 + rulePath(item, item.match).length;
		case "pattern": {
			item.match.pattern.lastIndex = 0;
			const match = item.match.pattern.exec(normalizePath(pathname));
			return 2_000_000 + (match?.[0].length ?? 0);
		}
		case "segment-prefix":
			return 1_000_000 + rulePath(item, item.match).length;
	}
}

interface ActiveCandidate {
	id: string;
	ancestors: string[];
	score: number;
}

function findCandidates(
	nodes: readonly AdminNavigationNode[],
	pathname: string,
	ancestors: string[],
	result: ActiveCandidate[],
): void {
	for (const node of nodes) {
		if (node.type === "item") {
			const score = matchScore(node, pathname);
			if (score >= 0) result.push({ id: node.id, ancestors, score });
		} else if (node.type === "group") {
			findCandidates(node.children, pathname, [...ancestors, node.id], result);
		}
	}
}

/** 解析唯一激活叶子及其递归祖先上的全部 group。 */
export function findActiveNavigationPath(
	sections: readonly NavigationSection[],
	pathname: string,
): ActiveNavigationPath {
	const candidates: ActiveCandidate[] = [];
	for (const section of sections) {
		findCandidates(section.children, pathname, [], candidates);
	}
	const active = candidates.sort((a, b) => b.score - a.score)[0];
	return active
		? { activeNodeId: active.id, ancestorNodeIds: active.ancestors }
		: { activeNodeId: null, ancestorNodeIds: [] };
}

function isVisible(
	isNodeVisible: AdminNavigationNode["isVisible"],
	context: NavigationVisibilityContext,
): boolean {
	return isNodeVisible?.(context) ?? true;
}

function compactSeparators(
	nodes: AdminNavigationNode[],
): AdminNavigationNode[] {
	const compact: AdminNavigationNode[] = [];
	for (const node of nodes) {
		if (node.type === "separator") {
			if (compact.length === 0 || compact.at(-1)?.type === "separator")
				continue;
		}
		compact.push(node);
	}
	if (compact.at(-1)?.type === "separator") compact.pop();
	return compact;
}

function filterNodes(
	nodes: readonly AdminNavigationNode[],
	context: NavigationVisibilityContext,
): AdminNavigationNode[] {
	const visible: AdminNavigationNode[] = [];
	for (const node of nodes) {
		if (!isVisible(node.isVisible, context)) continue;
		if (node.type !== "group") {
			visible.push(node);
			continue;
		}
		const children = filterNodes(node.children, context);
		if (children.length > 0) visible.push({ ...node, children });
	}
	return compactSeparators(visible);
}

/** 递归移除无权限节点、空 group 与空 section。 */
export function filterNavigationSections(
	sections: readonly NavigationSection[],
	context: NavigationVisibilityContext,
): NavigationSection[] {
	return sections.flatMap((section) => {
		if (!(section.isVisible?.(context) ?? true)) return [];
		const children = filterNodes(section.children, context);
		return children.length > 0 ? [{ ...section, children }] : [];
	});
}

function collectDefaultGroups(
	nodes: readonly AdminNavigationNode[],
	result: Set<string>,
): void {
	for (const node of nodes) {
		if (node.type !== "group") continue;
		if (node.defaultExpanded) result.add(node.id);
		collectDefaultGroups(node.children, result);
	}
}

export function defaultExpandedGroupIds(
	sections: readonly NavigationSection[],
): Set<string> {
	const result = new Set<string>();
	for (const section of sections)
		collectDefaultGroups(section.children, result);
	return result;
}

export function parseStoredExpandedIds(value: string | null): Set<string> {
	if (!value) return new Set();
	try {
		const parsed: unknown = JSON.parse(value);
		if (!Array.isArray(parsed)) return new Set();
		return new Set(parsed.filter((id): id is string => typeof id === "string"));
	} catch {
		return new Set();
	}
}

export function serializeExpandedIds(ids: ReadonlySet<string>): string {
	return JSON.stringify([...ids].sort());
}

export function groupDomId(groupId: string): string {
	return `admin-nav-group-${groupId.replace(/[^a-zA-Z0-9_-]/g, "-")}`;
}

export function isNavigationGroup(
	node: AdminNavigationNode,
): node is NavigationGroup {
	return node.type === "group";
}
