import { ProgressBar } from "@primer/react";
import { useEffect, useState } from "react";

import type { AwdEventStatus, AwdPlayerStatus } from "@/api/awd";

/** AWD 赛事阶段标签。 */
const PHASE_LABEL: Record<string, string> = {
	hardening: "Hardening",
	attack: "Attack",
	pause: "Pause",
};

/** AWD 赛事状态标签。 */
const STATUS_LABEL: Record<string, string> = {
	draft: "Draft",
	configuring: "Configuring",
	deploying: "Deploying",
	deployed: "Deployed",
	prechecking: "Prechecking",
	verified: "Verified",
	running: "Running",
	paused: "Paused",
	network_error: "Network Error",
	start_blocked: "Start Blocked",
	finished: "Finished",
	archived: "Archived",
	deploy_failed: "Deploy Failed",
	verification_failed: "Verification Failed",
};

const formatTime = (seconds: number) => {
	if (seconds <= 0) return "0s";
	const h = Math.floor(seconds / 3600);
	const m = Math.floor((seconds % 3600) / 60);
	const s = seconds % 60;
	const parts: string[] = [];
	if (h) parts.push(`${h}h`);
	if (m) parts.push(`${m}m`);
	if (s || parts.length === 0) parts.push(`${s}s`);
	return parts.join(" ");
};

export type AwdProgressState = {
	status: string;
	phase: string;
	currentRound: number | null;
	roundCount: number | null;
	roundDurationSecs: number;
	startedAt: string | null;
};

function useNow() {
	const [now, setNow] = useState(() => Date.now());
	useEffect(() => {
		const timer = setInterval(() => setNow(Date.now()), 1000);
		return () => clearInterval(timer);
	}, []);
	return now;
}

/**
 * AWD 赛事进度条组件。
 *
 * 放置于标题与 UnderlineNav 之间（对齐 Jeopardy RemainingTimer / AWDP AwdpEventProgress 位置）。
 * 接受 admin 或 player 状态，按 phase 显示进度文字与 ProgressBar。
 */
export function AwdEventProgress({
	status,
	phase,
	currentRound,
	roundCount,
	roundDurationSecs,
	startedAt,
}: AwdProgressState) {
	const now = useNow();
	const started = startedAt ? Date.parse(startedAt) : null;

	// ── 进度条百分比（简化：基于 phase 与 round） ──
	let progressPct = 100;
	let label = "";

	if (status === "finished" || status === "archived") {
		label = STATUS_LABEL[status] ?? status;
		progressPct = 0;
	} else if (status === "network_error") {
		label = "⚠ Network Error";
		progressPct = 100;
	} else if (phase === "pause") {
		label = "⏸ Paused";
		progressPct = 100;
	} else if (phase === "hardening") {
		label = "Hardening";
		progressPct = 100;
	} else if (phase === "attack") {
		const rn = currentRound ?? 1;
		const total = roundCount ?? 1;
		const elapsed = rn > 0 ? ((rn - 1) / total) * 100 : 0;
		label = `Attack — Round ${rn} / ${total}`;
		progressPct = 100 - elapsed;
	} else if (
		status === "draft" ||
		status === "configuring" ||
		status === "deploying" ||
		status === "deployed" ||
		status === "prechecking" ||
		status === "verified" ||
		status === "start_blocked" ||
		status === "deploy_failed" ||
		status === "verification_failed"
	) {
		label = STATUS_LABEL[status] ?? status;
		progressPct = 100;
	} else {
		label = STATUS_LABEL[status] ?? status;
		progressPct = 100;
	}

	return (
		<div className="mt-2">
			<div className="flex items-center justify-between text-sm tabular-nums text-[var(--fgColor-muted)]">
				<span>{label}</span>
				{phase === "attack" && currentRound != null && roundCount != null && (
					<span className="text-xs font-semibold uppercase tracking-wide">
						Round {currentRound} / {roundCount}
					</span>
				)}
			</div>
			<ProgressBar
				animated={status === "running" && phase === "attack"}
				progress={progressPct}
				aria-label="AWD event progress"
			/>
		</div>
	);
}

/**
 * 从 admin AWD 状态构建 AwdProgressState。
 */
export function adminProgressState(s: AwdEventStatus): AwdProgressState {
	return {
		status: s.status,
		phase: s.phase,
		currentRound: null, // admin status endpoint doesn't expose current round
		roundCount: s.round_count,
		roundDurationSecs: s.round_duration_secs,
		startedAt: s.started_at,
	};
}

/**
 * 从 player AWD 状态构建 AwdProgressState。
 */
export function playerProgressState(s: AwdPlayerStatus): AwdProgressState {
	return {
		status: s.status,
		phase: s.phase,
		currentRound: s.current_round,
		roundCount: s.round_count,
		roundDurationSecs: 0, // player status doesn't expose this
		startedAt: null,
	};
}