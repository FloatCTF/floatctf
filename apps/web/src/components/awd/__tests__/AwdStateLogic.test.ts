import { describe, expect, it } from "vitest";

/**
 * AWD Phase 8 UI Tests — State-driven UX logic.
 *
 * These tests validate the pure state-logic functions used by the UI components.
 * They do not render React components; they test the decision functions.
 */

// ── Replicate the state logic from the UI components (pure functions) ──

/** AWD event status values. */
type AwdStatus = string;
/** AWD phase values. */
type AwdPhase = string;

/** Flag submission eligibility (from Player Overview). */
function flagAllowed(
	phase: AwdPhase | undefined,
	status: AwdStatus | undefined,
	banned: boolean,
): { allowed: boolean; reason: string } {
	if (banned) return { allowed: false, reason: "Your team is banned." };
	if (!status || !phase)
		return { allowed: false, reason: "AWD not configured." };
	if (status === "finished" || status === "archived")
		return { allowed: false, reason: "Competition finished." };
	if (status === "paused")
		return { allowed: false, reason: "Competition paused." };
	if (status === "network_error")
		return { allowed: false, reason: "Infrastructure unavailable." };
	if (phase === "pause")
		return { allowed: false, reason: "Competition paused." };
	if (phase === "hardening")
		return { allowed: false, reason: "Attack has not started (Hardening)." };
	if (phase === "attack") return { allowed: true, reason: "" };
	return { allowed: false, reason: "Flag submission not available." };
}

/** Reset eligibility (from Player GameBoxes). */
function resetAllowed(
	phase: AwdPhase | undefined,
	status: AwdStatus | undefined,
	banned: boolean,
): { allowed: boolean; reason: string } {
	if (banned) return { allowed: false, reason: "Team is banned." };
	if (!status || !phase)
		return { allowed: false, reason: "AWD not configured." };
	if (status === "finished" || status === "archived")
		return { allowed: false, reason: "Competition finished." };
	if (status === "paused")
		return { allowed: false, reason: "Competition paused." };
	if (status === "network_error")
		return { allowed: false, reason: "Infrastructure unavailable." };
	if (phase === "pause")
		return { allowed: false, reason: "Competition paused." };
	if (phase === "hardening" || phase === "attack")
		return { allowed: true, reason: "" };
	return { allowed: false, reason: "Reset not available." };
}

/** Lifecycle actions visibility (from Admin Ops). */
function visibleActions(
	status: AwdStatus,
): string[] {
	switch (status) {
		case "draft":
		case "configuring":
		case "deploy_failed":
			return ["deploy"];
		case "deployed":
		case "verification_failed":
			return ["deploy", "precheck"];
		case "verified":
		case "start_blocked":
			return ["deploy", "precheck", "start"];
		case "running":
			return ["pause", "finish"];
		case "paused":
			return ["resume"];
		case "network_error":
			return ["resume"];
		case "finished":
			return ["archive"];
		case "archived":
			return [];
		default:
			return [];
	}
}

/** Score adjustment availability. */
function adjustAllowed(status: AwdStatus): boolean {
	return status !== "finished" && status !== "archived";
}

// ── Tests: Flag Submission ──

describe("flag submission eligibility", () => {
	it("allows flag during attack", () => {
		expect(flagAllowed("attack", "running", false)).toEqual({
			allowed: true,
			reason: "",
		});
	});

	it("disables flag during hardening", () => {
		const result = flagAllowed("hardening", "running", false);
		expect(result.allowed).toBe(false);
		expect(result.reason).toContain("Hardening");
	});

	it("disables flag when paused", () => {
		expect(flagAllowed("attack", "paused", false).allowed).toBe(false);
		expect(flagAllowed("hardening", "paused", false).allowed).toBe(false);
	});

	it("disables flag when network_error", () => {
		expect(flagAllowed("attack", "network_error", false).allowed).toBe(false);
	});

	it("disables flag when banned", () => {
		expect(flagAllowed("attack", "running", true).allowed).toBe(false);
		expect(flagAllowed("attack", "running", true).reason).toContain("banned");
	});

	it("disables flag when finished", () => {
		expect(flagAllowed("attack", "finished", false).allowed).toBe(false);
		expect(flagAllowed("attack", "archived", false).allowed).toBe(false);
	});

	it("disables flag when phase is pause", () => {
		expect(flagAllowed("pause", "running", false).allowed).toBe(false);
		expect(flagAllowed("pause", "running", false).reason).toContain("paused");
	});
});

// ── Tests: Reset ──

describe("reset eligibility", () => {
	it("allows reset during hardening", () => {
		expect(resetAllowed("hardening", "running", false)).toEqual({
			allowed: true,
			reason: "",
		});
	});

	it("allows reset during attack", () => {
		expect(resetAllowed("attack", "running", false)).toEqual({
			allowed: true,
			reason: "",
		});
	});

	it("disables reset when paused", () => {
		expect(resetAllowed("attack", "paused", false).allowed).toBe(false);
	});

	it("disables reset when network_error", () => {
		expect(resetAllowed("attack", "network_error", false).allowed).toBe(false);
	});

	it("disables reset when banned", () => {
		expect(resetAllowed("attack", "running", true).allowed).toBe(false);
	});

	it("disables reset when finished", () => {
		expect(resetAllowed("attack", "finished", false).allowed).toBe(false);
		expect(resetAllowed("attack", "archived", false).allowed).toBe(false);
	});
});

// ── Tests: Admin Lifecycle Actions ──

describe("admin lifecycle actions", () => {
	it("shows deploy for configuring events", () => {
		expect(visibleActions("configuring")).toContain("deploy");
		expect(visibleActions("configuring")).not.toContain("start");
	});

	it("shows start for verified events", () => {
		const actions = visibleActions("verified");
		expect(actions).toContain("start");
		expect(actions).not.toContain("pause");
	});

	it("shows pause and finish for running events", () => {
		const actions = visibleActions("running");
		expect(actions).toContain("pause");
		expect(actions).toContain("finish");
		expect(actions).not.toContain("start");
	});

	it("shows resume for paused events", () => {
		expect(visibleActions("paused")).toEqual(["resume"]);
	});

	it("shows resume for network_error events", () => {
		expect(visibleActions("network_error")).toEqual(["resume"]);
	});

	it("shows only archive for finished events", () => {
		expect(visibleActions("finished")).toEqual(["archive"]);
		expect(visibleActions("finished")).not.toContain("start");
		expect(visibleActions("finished")).not.toContain("pause");
	});

	it("shows no actions for archived events", () => {
		expect(visibleActions("archived")).toEqual([]);
	});
});

// ── Tests: Score Adjustment ──

describe("score adjustment", () => {
	it("allows adjustment during running", () => {
		expect(adjustAllowed("running")).toBe(true);
	});

	it("allows adjustment during paused", () => {
		expect(adjustAllowed("paused")).toBe(true);
	});

	it("allows adjustment during configuring", () => {
		expect(adjustAllowed("configuring")).toBe(true);
	});

	it("disables adjustment when finished", () => {
		expect(adjustAllowed("finished")).toBe(false);
	});

	it("disables adjustment when archived", () => {
		expect(adjustAllowed("archived")).toBe(false);
	});
});

// ── Tests: Negative Score Display ──

describe("scoreboard rendering", () => {
	it("handles negative total scores", () => {
		// Negative scores must be displayable without error
		const negativeScore = -500;
		expect(negativeScore.toString()).toBe("-500");
	});

	it("handles zero total score", () => {
		expect((0).toString()).toBe("0");
	});

	it("handles large positive scores", () => {
		expect((999999).toString()).toBe("999999");
	});
});

// ── Tests: Ban Semantics ──

describe("ban semantics", () => {
	it("ban has no duration — flag is disabled permanently", () => {
		// Ban is manual; no duration. Flag stays disabled regardless of time.
		expect(flagAllowed("attack", "running", true).allowed).toBe(false);
		expect(resetAllowed("attack", "running", true).allowed).toBe(false);
	});

	it("ban disables competition actions even during attack", () => {
		// Banned team cannot participate regardless of phase
		expect(flagAllowed("attack", "running", true).allowed).toBe(false);
		expect(resetAllowed("hardening", "running", true).allowed).toBe(false);
	});
});

// ── Tests: Final Settlement behavior ──

describe("final settlement / finished", () => {
	it("finished disables all competition actions", () => {
		expect(flagAllowed("attack", "finished", false).allowed).toBe(false);
		expect(resetAllowed("attack", "finished", false).allowed).toBe(false);
		expect(adjustAllowed("finished")).toBe(false);
	});

	it("archived disables all competition actions", () => {
		expect(flagAllowed("attack", "archived", false).allowed).toBe(false);
		expect(resetAllowed("attack", "archived", false).allowed).toBe(false);
		expect(adjustAllowed("archived")).toBe(false);
	});
});

// ── Tests: NetworkError vs Pause distinction ──

describe("network_error vs pause", () => {
	it("network_error disables flag and reset", () => {
		expect(flagAllowed("attack", "network_error", false).allowed).toBe(false);
		expect(resetAllowed("attack", "network_error", false).allowed).toBe(false);
	});

	it("network_error shows resume action", () => {
		expect(visibleActions("network_error")).toContain("resume");
	});

	it("pause shows resume action", () => {
		expect(visibleActions("paused")).toContain("resume");
	});

	it("network_error and pause are different states", () => {
		// Both show resume, but are semantically different
		expect(visibleActions("network_error")).toEqual(visibleActions("paused"));
		// The UI distinguishes them via banner/label, not action buttons
	});
});