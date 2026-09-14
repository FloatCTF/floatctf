import type {
	NavigationItem,
	NavigationSection,
	NavigationVisibilityContext,
} from "@/navigation/sidebar-types";
import {
	defaultExpandedGroupIds,
	filterNavigationSections,
	findActiveNavigationPath,
	matchNavigationItem,
	parseStoredExpandedIds,
	serializeExpandedIds,
} from "@/navigation/sidebar-utils";
import { describe, expect, it } from "vitest";

const item = (
	id: string,
	href: string,
	match: NavigationItem["match"],
): NavigationItem => ({
	type: "item",
	id,
	label: id,
	href,
	match,
});

const sections: NavigationSection[] = [
	{
		id: "s1",
		children: [
			{
				type: "group",
				id: "events",
				label: "Events",
				children: [
					item("events.all", "/admin/events", {
						mode: "pattern",
						pattern: /^\/admin\/events(?:\/(?:jeopardy|awd)\/[^/]+(?:\/.*)?)?$/,
					}),
					{
						type: "group",
						id: "events.awd",
						label: "AWD",
						children: [
							item("events.awd.gameboxes", "/admin/awd/gameboxes", {
								mode: "exact",
							}),
						],
					},
				],
			},
			item("docker", "/admin/docker", { mode: "segment-prefix" }),
		],
	},
];

describe("matchNavigationItem", () => {
	it("exact requires an identical normalized path", () => {
		const leaf = item("solo", "/admin/solo", { mode: "exact" });
		expect(matchNavigationItem(leaf, "/admin/solo")).toBe(true);
		expect(matchNavigationItem(leaf, "/admin/solo/")).toBe(true);
		expect(matchNavigationItem(leaf, "/admin/solo/x")).toBe(false);
		expect(matchNavigationItem(leaf, "/admin/solo-old")).toBe(false);
	});

	it("segment-prefix matches descendants but not sibling prefixes", () => {
		const docker = item("docker", "/admin/docker", { mode: "segment-prefix" });
		expect(matchNavigationItem(docker, "/admin/docker")).toBe(true);
		expect(matchNavigationItem(docker, "/admin/docker/images")).toBe(true);
		expect(matchNavigationItem(docker, "/admin/docker-old")).toBe(false);
	});

	it("pattern supports explicit descendant routes only", () => {
		const events = sections[0].children[0];
		if (events.type !== "group") throw new Error("fixture must be a group");
		const allEvents = events.children[0] as NavigationItem;
		expect(matchNavigationItem(allEvents, "/admin/events")).toBe(true);
		expect(matchNavigationItem(allEvents, "/admin/events/jeopardy/abc")).toBe(
			true,
		);
		expect(
			matchNavigationItem(allEvents, "/admin/events/awd/abc/configure"),
		).toBe(true);
		expect(matchNavigationItem(allEvents, "/admin/events/templates")).toBe(
			false,
		);
		expect(matchNavigationItem(allEvents, "/admin/events-old")).toBe(false);
	});
});

describe("findActiveNavigationPath", () => {
	it("returns the active leaf and its ancestor ids", () => {
		const active = findActiveNavigationPath(sections, "/admin/awd/gameboxes");
		expect(active.activeNodeId).toBe("events.awd.gameboxes");
		expect(active.ancestorNodeIds).toEqual(["events", "events.awd"]);
	});

	it("exact platform leaf does not shadow the event pattern", () => {
		const active = findActiveNavigationPath(
			sections,
			"/admin/events/awd/abc/ops",
		);
		expect(active.activeNodeId).toBe("events.all");
		expect(active.ancestorNodeIds).toEqual(["events"]);
	});

	it("returns no match for unknown routes", () => {
		const active = findActiveNavigationPath(sections, "/admin/whatever");
		expect(active.activeNodeId).toBeNull();
		expect(active.ancestorNodeIds).toEqual([]);
	});
});

describe("filterNavigationSections", () => {
	const canSee =
		(allowed: ReadonlySet<string>) => (context: NavigationVisibilityContext) =>
			context.permissions.has("admin") || allowed.size === 0;

	it("hides unauthorized leaves and drops empty groups/sections", () => {
		const permissioned: NavigationSection[] = [
			{
				id: "guard",
				children: [
					{
						type: "group",
						id: "secret-group",
						label: "Secret",
						children: [
							item("secret.a", "/admin/secret/a", { mode: "exact" }),
							item("secret.b", "/admin/secret/b", { mode: "exact" }),
						],
					},
					item("public", "/admin/public", { mode: "exact" }),
				],
				isVisible: canSee(new Set(["admin"])),
			},
		];

		const unauthorized = filterNavigationSections(permissioned, {
			permissions: new Set(),
			features: new Set(),
		});
		// 组内全部不可见 → 组与 section 都消失
		expect(unauthorized).toEqual([]);

		const authorized = filterNavigationSections(permissioned, {
			permissions: new Set(["admin"]),
			features: new Set(),
		});
		expect(authorized).toHaveLength(1);
		// 授权后组与公开 leaf 都保留
		expect(authorized[0].children).toHaveLength(2);
	});
});

describe("expansion persistence helpers", () => {
	it("default expanded groups are collected recursively", () => {
		const withDefault: NavigationSection[] = [
			{
				id: "s",
				children: [
					{
						type: "group",
						id: "outer",
						label: "Outer",
						defaultExpanded: true,
						children: [
							{
								type: "group",
								id: "inner",
								label: "Inner",
								children: [item("leaf", "/admin/x", { mode: "exact" })],
							},
						],
					},
				],
			},
		];
		expect(defaultExpandedGroupIds(withDefault)).toEqual(new Set(["outer"]));
	});

	it("serialize/parse round-trips sorted stable ids", () => {
		const stored = new Set(["b", "a", "c"]);
		expect(parseStoredExpandedIds(serializeExpandedIds(stored))).toEqual(
			new Set(["a", "b", "c"]),
		);
	});

	it("parse tolerates malformed or non-array storage", () => {
		expect(parseStoredExpandedIds(null)).toEqual(new Set());
		expect(parseStoredExpandedIds("not json")).toEqual(new Set());
		expect(parseStoredExpandedIds('{"a":1}')).toEqual(new Set());
		expect(parseStoredExpandedIds('["a", 2]')).toEqual(new Set(["a"]));
	});
});
