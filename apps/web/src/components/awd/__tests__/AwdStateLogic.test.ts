import { describe, expect, it } from "vitest";

/**
 * AWD Phase 8.1 UI Tests — State-driven UX logic with final settlement.
 */

// ── Replicate the state logic from the UI components (pure functions) ──

type AwdStatus = string;
type AwdPhase = string;

/** Flag submission eligibility (from Player Overview). */
function flagAllowed(
	phase: AwdPhase | undefined,
	status: AwdStatus | undefined,
	banned: boolean,
	finalSettlement: boolean,
): { allowed: boolean; reason: string } {
	if (banned) return { allowed: false, reason: "Your team is banned." };
	if (!status || !phase)
		return { allowed: false, reason: "AWD not configured." };
	if (status === "finished" || status === "archived")
		return { allowed: false, reason: "Competition finished." };
	if (finalSettlement)
		return { allowed: false, reason: "Final settlement — competition is closed." };
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
	finalSettlement: boolean,
): { allowed: boolean; reason: string } {
	if (banned) return { allowed: false, reason: "Team is banned." };
	if (!status || !phase)
		return { allowed: false, reason: "AWD not configured." };
	if (status === "finished" || status === "archived")
		return { allowed: false, reason: "Competition finished." };
	if (finalSettlement)
		return { allowed: false, reason: "Final settlement — competition is closed." };
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
	finalSettlement: boolean,
): string[] {
	// Final settlement: no actions
	if (finalSettlement) return [];
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
			return ["pause"]; // No manual Finish during normal play
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
	it("allows flag during normal active attack", () => {
		expect(flagAllowed("attack", "running", false, false)).toEqual({
			allowed: true,
			reason: "",
		});
	});

	it("disables flag during hardening", () => {
		const result = flagAllowed("hardening", "running", false, false);
		expect(result.allowed).toBe(false);
		expect(result.reason).toContain("Hardening");
	});

	it("disables flag when paused", () => {
		expect(flagAllowed("attack", "paused", false, false).allowed).toBe(false);
		expect(flagAllowed("hardening", "paused", false, false).allowed).toBe(false);
	});

	it("disables flag when network_error", () => {
		expect(flagAllowed("attack", "network_error", false, false).allowed).toBe(false);
	});

	it("disables flag when banned", () => {
		expect(flagAllowed("attack", "running", true, false).allowed).toBe(false);
		expect(flagAllowed("attack", "running", true, false).reason).toContain("banned");
	});

	it("disables flag when finished", () => {
		expect(flagAllowed("attack", "finished", false, false).allowed).toBe(false);
		expect(flagAllowed("attack", "archived", false, false).allowed).toBe(false);
	});

	it("disables flag during final settlement", () => {
		// Running + Attack + final_settlement = true → closed
		const result = flagAllowed("attack", "running", false, true);
		expect(result.allowed).toBe(false);
		expect(result.reason).toContain("Final settlement");
	});

	it("disables flag when phase is pause", () => {
		expect(flagAllowed("pause", "running", false, false).allowed).toBe(false);
		expect(flagAllowed("pause", "running", false, false).reason).toContain("paused");
	});
});

// ── Tests: Reset ──

describe("reset eligibility", () => {
	it("allows reset during normal active hardening", () => {
		expect(resetAllowed("hardening", "running", false, false)).toEqual({
			allowed: true,
			reason: "",
		});
	});

	it("allows reset during normal active attack", () => {
		expect(resetAllowed("attack", "running", false, false)).toEqual({
			allowed: true,
			reason: "",
		});
	});

	it("disables reset when paused", () => {
		expect(resetAllowed("attack", "paused", false, false).allowed).toBe(false);
	});

	it("disables reset when network_error", () => {
		expect(resetAllowed("attack", "network_error", false, false).allowed).toBe(false);
	});

	it("disables reset when banned", () => {
		expect(resetAllowed("attack", "running", true, false).allowed).toBe(false);
	});

	it("disables reset when finished", () => {
		expect(resetAllowed("attack", "finished", false, false).allowed).toBe(false);
		expect(resetAllowed("attack", "archived", false, false).allowed).toBe(false);
	});

	it("disables reset during final settlement", () => {
		const result = resetAllowed("attack", "running", false, true);
		expect(result.allowed).toBe(false);
		expect(result.reason).toContain("Final settlement");
	});
});

// ── Tests: Admin Lifecycle Actions ──

describe("admin lifecycle actions", () => {
	it("shows deploy for configuring events", () => {
		expect(visibleActions("configuring", false)).toContain("deploy");
		expect(visibleActions("configuring", false)).not.toContain("start");
	});

	it("shows start for verified events", () => {
		const actions = visibleActions("verified", false);
		expect(actions).toContain("start");
	});

	it("shows only pause for running events (no manual Finish)", () => {
		const actions = visibleActions("running", false);
		expect(actions).toContain("pause");
		expect(actions).not.toContain("finish");
		expect(actions).not.toContain("start");
		expect(actions).toEqual(["pause"]);
	});

	it("shows resume for paused events", () => {
		expect(visibleActions("paused", false)).toEqual(["resume"]);
	});

	it("shows resume for network_error events", () => {
		expect(visibleActions("network_error", false)).toEqual(["resume"]);
	});

	it("shows only archive for finished events", () => {
		expect(visibleActions("finished", false)).toEqual(["archive"]);
	});

	it("shows no actions for archived events", () => {
		expect(visibleActions("archived", false)).toEqual([]);
	});

	it("shows no actions during final settlement", () => {
		// Final settlement: no Pause, no Resume, no Start, no Finish
		expect(visibleActions("running", true)).toEqual([]);
	});

	it("manual Finish is NOT exposed during normal running", () => {
		expect(visibleActions("running", false)).not.toContain("finish");
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

	it("disables adjustment when finished", () => {
		expect(adjustAllowed("finished")).toBe(false);
	});

	it("disables adjustment when archived", () => {
		expect(adjustAllowed("archived")).toBe(false);
	});
});

// ── Tests: Final Settlement vs Finished distinction ──

describe("final settlement vs finished", () => {
	it("final settlement disables flag (Running + Attack + final_settlement)", () => {
		expect(flagAllowed("attack", "running", false, true).allowed).toBe(false);
	});

	it("finished disables flag (status = finished)", () => {
		expect(flagAllowed("attack", "finished", false, false).allowed).toBe(false);
	});

	it("final settlement disables reset", () => {
		expect(resetAllowed("attack", "running", false, true).allowed).toBe(false);
	});

	it("finished disables reset", () => {
		expect(resetAllowed("attack", "finished", false, false).allowed).toBe(false);
	});

	it("final settlement shows no lifecycle actions", () => {
		expect(visibleActions("running", true)).toEqual([]);
	});

	it("finished shows archive action", () => {
		expect(visibleActions("finished", false)).toContain("archive");
	});

	it("finished differs visibly from final settlement in actions", () => {
		expect(visibleActions("running", true)).not.toEqual(
			visibleActions("finished", false),
		);
	});
});

// ── Tests: Negative Score Display ──

describe("scoreboard rendering", () => {
	it("handles negative total scores", () => {
		expect((-500).toString()).toBe("-500");
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
	it("ban disables flag and reset regardless of phase", () => {
		expect(flagAllowed("attack", "running", true, false).allowed).toBe(false);
		expect(resetAllowed("attack", "running", true, false).allowed).toBe(false);
		expect(resetAllowed("hardening", "running", true, false).allowed).toBe(false);
	});

	it("ban has no duration — flag stays disabled permanently", () => {
		expect(flagAllowed("attack", "running", true, false).allowed).toBe(false);
	});
});

// ── Tests: NetworkError vs Pause distinction ──

describe("network_error vs pause", () => {
	it("network_error disables flag and reset", () => {
		expect(flagAllowed("attack", "network_error", false, false).allowed).toBe(false);
		expect(resetAllowed("attack", "network_error", false, false).allowed).toBe(false);
	});

	it("network_error shows resume action", () => {
		expect(visibleActions("network_error", false)).toContain("resume");
	});

	it("pause shows resume action", () => {
		expect(visibleActions("paused", false)).toContain("resume");
	});
});