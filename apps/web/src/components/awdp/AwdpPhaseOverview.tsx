/**
 * AWDP 顶部状态面板（Phase / Countdown / Score / Break→Fix Timeline）。
 *
 * 设计约束（玩家侧重设计 spec）：
 *  - Practice 与 Competition 共用同一组件（不复制两套 header）；
 *  - 视觉：compact、thin-border、信息密度高但不拥挤，沿用 FloatCTF
 *    Primer design tokens（--accent-fg / --attention-fg / --success-fg /
 *    --fgColor-muted / --borderColor-*），不引入新 UI 依赖；
 *  - Timeline 宽度按真实 duration 比例（break : fix），Fix 段按
 *    totalRounds 均分绘制 Turn 分隔线，marker 由真实 timestamps 计算；
 *  - 时间格式 MM:SS / H:MM:SS（tabular-nums 防抖动），禁止 "0h 45m 42s"；
 *  - 纯展示组件：数据由调用方（AwdpWorkbench）传入，无内部 API/轮询。
 *
 * 不变量：绝不在本组件内展示 exploit 内容/路径、source object key、credentials。
 */
import { Label } from "@primer/react";

import type { AwdpPhase } from "@/api/awdp";

// ────────────────────────────────────────────────────────────────────────────
// Phase 语义元数据（badge 颜色/描述文案）
// ────────────────────────────────────────────────────────────────────────────

export type AwdpPhaseMeta = {
	text: string;
	variant: "attention" | "accent" | "success" | "done" | "secondary";
	description: string;
};

export const PHASE_META: Record<string, AwdpPhaseMeta> = {
	pending: {
		text: "NOT STARTED",
		variant: "secondary",
		description: "Waiting for the AWDP run to begin",
	},
	break: {
		text: "BREAK",
		variant: "accent",
		description: "Exploit the target and submit the flag",
	},
	fix: {
		text: "FIX",
		variant: "attention",
		description: "Patch the service before the next evaluation",
	},
	ended: {
		text: "ENDED",
		variant: "done",
		description: "This AWDP run has finished",
	},
};

// ────────────────────────────────────────────────────────────────────────────
// 格式化工具（导出供测试）
// ────────────────────────────────────────────────────────────────────────────

/** 倒计时：<1h → MM:SS，>=1h → H:MM:SS。null/非法 → "-"。 */
export function formatCountdown(seconds: number | null | undefined): string {
	if (seconds == null || !Number.isFinite(seconds)) {
		return "-";
	}
	const s = Math.max(0, Math.ceil(seconds));
	const h = Math.floor(s / 3600);
	const m = Math.floor((s % 3600) / 60);
	const sec = s % 60;
	const mm = String(m).padStart(2, "0");
	const ss = String(sec).padStart(2, "0");
	return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

/** 时长：45m / 1h / 1h 30m。 */
export function formatDuration(seconds: number | null | undefined): string {
	if (seconds == null || !Number.isFinite(seconds)) {
		return "-";
	}
	const minutes = Math.round(Math.max(0, seconds) / 60);
	if (minutes < 60) {
		return `${minutes}m`;
	}
	const h = Math.floor(minutes / 60);
	const rem = minutes % 60;
	return rem === 0 ? `${h}h` : `${h}h ${rem}m`;
}

/** 分数千分位：1000 → "1,000"。 */
export function formatScore(n: number): string {
	return new Intl.NumberFormat("en-US").format(n);
}

// ────────────────────────────────────────────────────────────────────────────
// Timeline 纯计算（导出供测试；now 用 Date.now()）
// ────────────────────────────────────────────────────────────────────────────

export type AwdpTimelineInput = {
	phase: AwdpPhase;
	breakDurationSecs: number;
	fixDurationSecs: number;
	totalRounds: number;
	/** Break 实际开始时间（ISO）。pending 为 null。 */
	startedAt?: string | null;
	/** Fix 实际开始时间（ISO）。非 fix/ended 为 null。 */
	fixStartedAt?: string | null;
	now: number;
};

export type AwdpTimelineState = {
	totalSecs: number;
	/** Break 段占全宽百分比（按 duration 比例，非 50/50）。 */
	breakWidthPct: number;
	fixWidthPct: number;
	/** 整体进度 0..1（break 用 now-startedAt，fix 用 break+now-fixStartedAt）。 */
	progress: number;
	/** marker 绝对位置 0..100。 */
	markerPct: number;
	/** Break 段内已完成百分比（0..100）。 */
	breakFillPct: number;
	/** Fix 段内已完成百分比（0..100）。 */
	fixFillPct: number;
	/** Turn 分隔线绝对位置（不含 0/100 端点）。 */
	turnBoundariesPct: number[];
	/** Break 已过去秒数（label "14:18 / 1h" 用；无数据时 null）。 */
	elapsedBreakSecs: number | null;
};

export function computeTimelineState(
	input: AwdpTimelineInput,
): AwdpTimelineState {
	const {
		phase,
		breakDurationSecs,
		fixDurationSecs,
		totalRounds,
		startedAt,
		fixStartedAt,
		now,
	} = input;
	const b = Math.max(0, breakDurationSecs);
	const f = Math.max(0, fixDurationSecs);
	const total = b + f;
	const breakWidthPct = total > 0 ? (b / total) * 100 : 50;
	const fixWidthPct = total > 0 ? (f / total) * 100 : 50;

	const clamp01 = (v: number) => Math.min(1, Math.max(0, v));
	const since = (iso?: string | null) => {
		if (!iso) {
			return null;
		}
		const t = Date.parse(iso);
		if (!Number.isFinite(t)) {
			return null;
		}
		return Math.max(0, (now - t) / 1000);
	};

	let elapsed = 0; // 总 elapsed 秒
	let breakElapsed: number | null = null;
	let breakFill = 0;
	let fixFill = 0;

	switch (phase) {
		case "pending":
			break;
		case "break": {
			breakElapsed = since(startedAt);
			const be = breakElapsed ?? 0;
			breakFill = b > 0 ? clamp01(be / b) * 100 : 100;
			elapsed = be;
			break;
		}
		case "fix": {
			breakElapsed = since(startedAt);
			const be = breakElapsed ?? 0;
			const fe = since(fixStartedAt) ?? 0;
			breakFill = 100;
			fixFill = f > 0 ? clamp01(fe / f) * 100 : 100;
			elapsed = b + fe;
			break;
		}
		case "ended":
			breakFill = 100;
			fixFill = 100;
			elapsed = total;
			break;
	}

	const progress = total > 0 ? clamp01(elapsed / total) : 0;
	const n = Math.max(1, Math.round(totalRounds));
	const turnBoundariesPct = Array.from(
		{ length: Math.max(0, n - 1) },
		(_, i) => breakWidthPct + ((i + 1) / n) * fixWidthPct,
	);

	return {
		totalSecs: total,
		breakWidthPct,
		fixWidthPct,
		progress,
		markerPct: progress * 100,
		breakFillPct: breakFill,
		fixFillPct: fixFill,
		turnBoundariesPct,
		elapsedBreakSecs: breakElapsed,
	};
}

// ────────────────────────────────────────────────────────────────────────────
// Timeline
// ────────────────────────────────────────────────────────────────────────────

type AwdpTimelineProps = {
	state: AwdpTimelineState;
	phase: AwdpPhase;
	breakDurationSecs: number;
	fixDurationSecs: number;
	totalRounds: number;
	/** Fix 时：显示 "Turn X / Y"；Ended：显示完成 turns。 */
	currentRound: number;
};

/** 8px track，Break/Fix 双段按 duration 比例，Fix 段内均分 Turn 分隔线。 */
function AwdpTimeline({
	state,
	phase,
	breakDurationSecs,
	fixDurationSecs,
	totalRounds,
	currentRound,
}: AwdpTimelineProps) {
	const markerColor =
		phase === "break"
			? "border-[var(--accent-fg)]"
			: phase === "fix"
				? "border-[var(--attention-fg)]"
				: phase === "ended"
					? "border-[var(--success-fg)]"
					: "border-[var(--fgColor-muted)]";

	// 底部 label（§12-14 语义）
	let leftLabel: string;
	let rightLabel: string;
	if (phase === "break") {
		leftLabel =
			state.elapsedBreakSecs != null
				? `${formatCountdown(state.elapsedBreakSecs)} / ${formatDuration(breakDurationSecs)}`
				: formatDuration(breakDurationSecs);
		rightLabel = `${formatDuration(fixDurationSecs)} Fix · ${totalRounds} turns`;
	} else if (phase === "fix") {
		leftLabel = "Completed";
		rightLabel = `Turn ${currentRound} / ${totalRounds}`;
	} else if (phase === "ended") {
		leftLabel = "Completed";
		rightLabel = `${totalRounds} / ${totalRounds} turns`;
	} else {
		leftLabel = `${formatDuration(breakDurationSecs)} Break`;
		rightLabel = `${formatDuration(fixDurationSecs)} Fix · ${totalRounds} turns`;
	}

	const showTurnLabels =
		(phase === "fix" || phase === "ended") &&
		totalRounds > 0 &&
		totalRounds <= 8;
	const showTurnSeparators =
		phase !== "break" && state.turnBoundariesPct.length > 0;

	return (
		<div>
			{/* 段名 */}
			<div className="flex items-center justify-between text-[11px] mb-1">
				<span
					className={
						phase === "break"
							? "font-semibold text-[var(--fgColor-default)]"
							: "font-semibold text-[var(--fgColor-muted)]"
					}
				>
					Break
				</span>
				<span
					className={
						phase === "fix" || phase === "ended"
							? "font-semibold text-[var(--fgColor-default)]"
							: "font-semibold text-[var(--fgColor-muted)]"
					}
				>
					Fix
				</span>
			</div>

			{/* track */}
			<div
				role="progressbar"
				tabIndex={0}
				aria-label={`AWDP progress: ${phase} phase`}
				aria-valuemin={0}
				aria-valuemax={100}
				aria-valuenow={Math.round(state.progress * 100)}
				className="relative h-1.5 rounded-full bg-[var(--borderColor-default)] overflow-hidden"
			>
				{/* Break 段（按比例宽度，填充为已完成部分） */}
				<div
					className="absolute inset-y-0 left-0 transition-[width] duration-200 ease-out motion-reduce:transition-none"
					style={{ width: `${state.breakWidthPct}%` }}
				>
					<div
						className="h-full bg-[var(--accent-fg)] transition-[width] duration-200 ease-out motion-reduce:transition-none"
						style={{ width: `${state.breakFillPct}%` }}
					/>
				</div>

				{/* Fix 段（含 Turn 分隔线） */}
				<div
					className="absolute inset-y-0 border-l-2 border-[var(--borderColor-default)] transition-[width] duration-200 ease-out motion-reduce:transition-none"
					style={{
						left: `${state.breakWidthPct}%`,
						width: `${state.fixWidthPct}%`,
					}}
				>
					<div
						className="h-full bg-[var(--attention-fg)] transition-[width] duration-200 ease-out motion-reduce:transition-none"
						style={{ width: `${state.fixFillPct}%` }}
					/>
					{showTurnSeparators &&
						state.turnBoundariesPct.map((pct) => (
							<div
								key={pct}
								aria-hidden="true"
								className="absolute inset-y-0 w-px bg-[var(--bgColor-default)]"
								style={{ left: `${pct - state.breakWidthPct}%` }}
							/>
						))}
				</div>

				{/* marker */}
				<div
					aria-hidden="true"
					className={`absolute top-1/2 h-2 w-2 -translate-x-1/2 -translate-y-1/2 rounded-full bg-white border-2 ${markerColor} transition-[left] duration-200 ease-out motion-reduce:transition-none`}
					style={{ left: `${state.markerPct}%` }}
				/>
			</div>

			{/* Turn labels（<=8 turns 才显示文字，避免拥挤） */}
			{showTurnLabels && (
				<div className="relative h-3.5 mt-0.5 text-[10px] leading-3.5 text-[var(--fgColor-muted)] tabular-nums select-none">
					{Array.from({ length: totalRounds }, (_, i) => {
						const left =
							state.breakWidthPct +
							((i + 0.5) / totalRounds) * state.fixWidthPct;
						return (
							<span
								key={left}
								aria-hidden="true"
								className="absolute -translate-x-1/2"
								style={{ left: `${left}%` }}
							>
								T{i + 1}
							</span>
						);
					})}
				</div>
			)}

			{/* 底部 label */}
			<div className="flex items-center justify-between text-[11px] mt-1">
				<span className="text-[var(--fgColor-muted)] tabular-nums">
					{leftLabel}
				</span>
				<span className="text-[var(--fgColor-muted)] tabular-nums">
					{rightLabel}
				</span>
			</div>
		</div>
	);
}

// ────────────────────────────────────────────────────────────────────────────
// Phase Overview（顶部状态面板）
// ────────────────────────────────────────────────────────────────────────────

export type AwdpPhaseOverviewProps = {
	phase: AwdpPhase;
	/** Break 开始时间（ISO）。 */
	startedAt?: string | null;
	breakEndsAt?: string | null;
	fixStartedAt?: string | null;
	fixEndsAt?: string | null;
	breakDurationSecs: number;
	fixDurationSecs: number;
	currentRound: number;
	totalRounds: number;
	/** Fix 阶段：下一 cutoff 时间（ISO）。 */
	nextCheckAt?: string | null;
	score: number;
	/** 当前时间戳（Date.now()，由调用方 1s 刷新）。 */
	now: number;
};

/**
 * 顶部状态面板：Phase badge+描述（左）| SCORE（右）→ Countdown → Timeline。
 * 视线顺序：Phase → 剩余时间 → Turn/Next check → 得分（§21）。
 */
export function AwdpPhaseOverview(props: AwdpPhaseOverviewProps) {
	const {
		phase,
		startedAt,
		breakEndsAt,
		fixStartedAt,
		fixEndsAt,
		breakDurationSecs,
		fixDurationSecs,
		currentRound,
		totalRounds,
		nextCheckAt,
		score,
		now,
	} = props;
	const meta = PHASE_META[phase] ?? PHASE_META.pending;
	const state = computeTimelineState({
		phase,
		breakDurationSecs,
		fixDurationSecs,
		totalRounds,
		startedAt,
		fixStartedAt,
		now,
	});

	const secsUntil = (iso?: string | null) => {
		if (!iso) {
			return null;
		}
		const t = Date.parse(iso);
		if (!Number.isFinite(t)) {
			return null;
		}
		return Math.max(0, (t - now) / 1000);
	};

	// ── Countdown 区（§7：Break 主倒计时 / Fix 主 Next check、次 Fix ends）──
	let countdownText = "-";
	let countdownCaption = "remaining";
	let countdownSecondary: string | null = null;

	if (phase === "break") {
		countdownText = formatCountdown(secsUntil(breakEndsAt));
		countdownCaption = "remaining";
	} else if (phase === "fix") {
		const next = secsUntil(nextCheckAt);
		const fixEnd = secsUntil(fixEndsAt);
		if (next == null) {
			countdownText = "Pending";
			countdownCaption = "next check";
		} else {
			countdownText = formatCountdown(next);
			countdownCaption = "next check in";
		}
		countdownSecondary =
			fixEnd != null ? `Fix ends in ${formatCountdown(fixEnd)}` : null;
	} else if (phase === "ended") {
		countdownText = "Finished";
		countdownCaption = "";
	} else {
		// pending：无具体 start 时间 → Waiting to start（§15/§31）
		countdownText = "Waiting to start";
		countdownCaption = "";
	}

	return (
		<section className="rounded border px-3 py-2">
			{/* 第一行：Phase identity（左）| Score（右） */}
			<div className="flex items-start justify-between gap-4 flex-wrap">
				<div className="min-w-0">
					<div className="flex items-center gap-2 flex-wrap">
						<Label variant={meta.variant}>{meta.text}</Label>
						{phase === "fix" && (
							<span className="text-xs font-semibold uppercase tracking-wide text-[var(--fgColor-muted)] tabular-nums">
								Turn {currentRound} / {totalRounds}
							</span>
						)}
					</div>
					<p className="mt-0.5 text-xs text-[var(--fgColor-muted)]">
						{meta.description}
					</p>
				</div>
				<div className="text-right shrink-0">
					<p className="text-[11px] font-semibold uppercase tracking-wide text-[var(--fgColor-muted)]">
						{phase === "ended" ? "Final Score" : "Score"}
					</p>
					<p className="text-xl leading-4 font-semibold tabular-nums text-[var(--fgColor-default)]">
						{formatScore(score)}
					</p>
				</div>
			</div>

			{/* 第二行：Countdown */}
			<div className="mt-1.5">
				<p className="text-xl leading-4 font-semibold tabular-nums text-[var(--fgColor-default)]">
					{countdownText}
				</p>
				{countdownCaption ? (
					<p className="text-xs text-[var(--fgColor-muted)]">
						{countdownCaption}
					</p>
				) : null}
				{countdownSecondary ? (
					<p className="mt-1 text-xs text-[var(--fgColor-muted)] tabular-nums">
						{countdownSecondary}
					</p>
				) : null}
			</div>

			{/* 第三行：Break → Fix Timeline */}
			<div className="mt-1.5">
				<AwdpTimeline
					state={state}
					phase={phase}
					breakDurationSecs={breakDurationSecs}
					fixDurationSecs={fixDurationSecs}
					totalRounds={totalRounds}
					currentRound={currentRound}
				/>
			</div>
		</section>
	);
}
