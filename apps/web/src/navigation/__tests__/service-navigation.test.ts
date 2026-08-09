import { serviceNavigation } from "@/navigation/service-navigation";
import type {
	AdminNavigationNode,
	NavigationItem,
} from "@/navigation/sidebar-types";
import {
	findActiveNavigationPath,
	matchNavigationItem,
} from "@/navigation/sidebar-utils";
import { describe, expect, it } from "vitest";

function collectLeaves(
	nodes: readonly AdminNavigationNode[],
	result: NavigationItem[] = [],
): NavigationItem[] {
	for (const node of nodes) {
		if (node.type === "item") result.push(node);
		else if (node.type === "group") collectLeaves(node.children, result);
	}
	return result;
}

describe("serviceNavigation", () => {
	it("every leaf id is unique", () => {
		const ids = collectLeaves(serviceNavigation.flatMap((s) => s.children)).map(
			(leaf) => leaf.id,
		);
		expect(new Set(ids).size).toBe(ids.length);
	});

	it("hrefs cover every player page exactly once", () => {
		const hrefs = collectLeaves(
			serviceNavigation.flatMap((s) => s.children),
		).map((leaf) => leaf.href);
		expect([...hrefs].sort()).toEqual(
			[
				"/service/top",
				"/service/events",
				"/service/challenges",
				"/service/challenge_sets",
				"/service/solves",
				"/service/instances",
				"/service/announcements",
				"/service/discussions",
				"/service/writeups",
				"/service/weapons",
				"/service/profile",
			].sort(),
		);
	});

	it("landing section is label-less and leads to the /service redirect target", () => {
		const landing = serviceNavigation[0];
		expect(landing.label).toBeUndefined();
		const leaf = collectLeaves(landing.children)[0];
		expect(leaf.href).toBe("/service/top");
	});

	it("detail routes activate their segment-prefix leaf only", () => {
		const active = (pathname: string) =>
			findActiveNavigationPath(serviceNavigation, pathname).activeNodeId;

		expect(active("/service/events/jeopardy/abc/challenges")).toBe(
			"service.events",
		);
		expect(active("/service/challenges/abc/writeup")).toBe(
			"service.challenges",
		);
		expect(active("/service/challenge_sets/abc")).toBe(
			"service.challenge-sets",
		);
		expect(active("/service/discussions/my")).toBe("service.discussions");
		expect(active("/service/writeups/abc")).toBe("service.writeups");
	});

	it("segment boundaries separate challenges from challenge_sets", () => {
		const challenges = collectLeaves(
			serviceNavigation.flatMap((s) => s.children),
		).find((leaf) => leaf.href === "/service/challenges");
		const challengeSets = collectLeaves(
			serviceNavigation.flatMap((s) => s.children),
		).find((leaf) => leaf.href === "/service/challenge_sets");
		if (!challenges || !challengeSets)
			throw new Error("fixture leaves missing");

		expect(matchNavigationItem(challenges, "/service/challenge_sets")).toBe(
			false,
		);
		expect(matchNavigationItem(challengeSets, "/service/challenges")).toBe(
			false,
		);
		expect(matchNavigationItem(challenges, "/service/challenges/abc")).toBe(
			true,
		);
	});

	it("unmatched player paths leave no active leaf", () => {
		expect(
			findActiveNavigationPath(serviceNavigation, "/service").activeNodeId,
		).toBeNull();
		expect(
			findActiveNavigationPath(serviceNavigation, "/service/whatever")
				.activeNodeId,
		).toBeNull();
	});
});
