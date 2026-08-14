/**
 * AWDP 顶部状态卡片（Practice 与 Competition 共用，不复制两套 header）。
 *
 * 设计约束：
 *  - 紧凑单卡片：Row 1 阶段控制 | Row 2 事件式倒计时 + 得分 | Row 3 Break→Fix 时间线；
 *  - 练习模式（canControlPhase）：Row 1 左侧 <SegmentedControl>（左 Break / 右 Fix）
 *    切换阶段，space-between 最右侧为「End」；竞赛模式显示 Phase badge；
 *  - 倒计时文案与 event 赛事 RemainingTimer 同款（"Ends in: 45m 12s"），
 *    tabular-nums 防抖动；
 *  - Timeline 宽度按真实 duration 比例（break : fix），Fix 段按 totalRounds
 *    均分 Turn 分隔线，marker 由真实 timestamps 计算；已走过（elapsed）填充
 *    带 event 赛事同款动画条纹（.awdp-stripes）；
 *  - 纯展示组件：数据/busy/回调由调用方（AwdpWorkbench）传入，无内部 API/轮询。
 *
 * 不变量：绝不在本组件内展示 exploit 内容/路径、source object key、credentials。
 */
import { BugIcon, ToolsIcon } from "@primer/octicons-react";
import { Button, Label, SegmentedControl } from "@primer/react";

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

/**
 * event 赛事 RemainingTimer 同款剩余时间文案：d/h/m/s 分段（"1h 23m 45s" /
 * "45m 12s" / "45s"）。null/非法 → "-"。
 */
export function formatEventRemaining(
	seconds: number | null | undefined,
): string {
	if (seconds == null || !Number.isFinite(seconds)) {
		return "-";
	}
	const s = Math.max(0, Math.floor(seconds));
	const days = Math.floor(s / 86400);
	const hours = Math.floor((s % 86400) / 3600);
	const minutes = Math.floor((s % 3600) / 60);
	const sec = s % 60;
	const parts: string[] = [];
	if (days) {
		parts.push(`${days}d`);
	}
	if (hours) {
		parts.push(`${hours}h`);
	}
	if (minutes) {
		parts.push(`${minutes}m`);
	}
	if (sec || parts.length === 0) {
		parts.push(`${sec}s`);
	}
	return parts.join(" ");
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
	/** Break 已过去秒数（label 用；无数据时 null）。 */
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
// Timeline（紧凑版：仅段名 + track；Turn 分隔线保留在 track 内，去掉 T 文字行
// 与底部 label 行以压缩高度）
// ────────────────────────────────────────────────────────────────────────────

type AwdpTimelineProps = {
	state: AwdpTimelineState;
	phase: AwdpPhase;
	totalRounds: number;
};

/** 紧凑 track，Break/Fix 双段按 duration 比例，Fix 段内均分 Turn 分隔线。 */
export function AwdpTimeline({ state, phase, totalRounds }: AwdpTimelineProps) {
	// 进度条已走过部分整体为绿色（success），marker 跟随。
	const markerColor =
		phase === "pending"
			? "border-[var(--fgColor-muted)]"
			: "border-[var(--fgColor-success)]";

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
				{/* Break 段（按比例宽度，填充为已完成部分，带 event 同款动画条纹） */}
				<div
					className="absolute inset-y-0 left-0 transition-[width] duration-200 ease-out motion-reduce:transition-none"
					style={{ width: `${state.breakWidthPct}%` }}
				>
					<div
						className="awdp-stripes h-full bg-[var(--fgColor-success)] transition-[width] duration-200 ease-out motion-reduce:transition-none"
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
						className="awdp-stripes h-full bg-[var(--fgColor-success)] transition-[width] duration-200 ease-out motion-reduce:transition-none"
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

			{/* 时间节点：Fix/Ended 且回合数 <= 8 时显示 T1..Tn（每个 Turn 起点/段中心），避免拥挤 */}
			{(phase === "fix" || phase === "ended") &&
				totalRounds > 0 &&
				totalRounds <= 8 && (
					<div className="relative mt-0.5 h-3.5 text-[10px] leading-none text-[var(--fgColor-muted)] tabular-nums select-none">
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
		</div>
	);
}

// ────────────────────────────────────────────────────────────────────────────
// Phase Overview（顶部状态卡片）
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
	/** 练习模式：Row 1 左侧显示 <SegmentedControl> 阶段控制（左 Break / 右 Fix）。 */
	canControlPhase?: boolean;
	/** 阶段切换回调（练习模式，target break|fix）。 */
	onSetPhase?: (target: "break" | "fix") => void | Promise<void>;
	/** 阶段切换进行中（禁用 SegmentedControl）。 */
	phaseBusy?: boolean;
	/** 练习「End」：Row 1 最右侧（space-between）。 */
	onEnd?: () => void | Promise<void>;
	/** End 进行中（仅 End 操作本身，不含阶段切换）：控制文案“停止中…”。 */
	endRunning?: boolean;
	/** End 按钮禁用条件：End 操作进行中或页面级阶段切换阻塞中。 */
	endBusy?: boolean;
};

/**
 * 顶部状态卡片：Row 1 阶段控制（练习 SegmentedControl / 竞赛 badge）| End →
 * Row 2 事件式倒计时 + 得分（压缩单行）→ Row 3 Break→Fix 时间线。
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
		canControlPhase,
		onSetPhase,
		phaseBusy,
		onEnd,
		endRunning,
		endBusy,
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

	const practice = !!canControlPhase && !!onSetPhase;
	const controllable = practice && (phase === "break" || phase === "fix");

	// ── 事件式倒计时文案（与 event 赛事 RemainingTimer 同款）──
	let countdownText = "Waiting to start";
	if (phase === "break") {
		countdownText = `Ends in: ${formatEventRemaining(secsUntil(breakEndsAt))}`;
	} else if (phase === "fix") {
		countdownText = `Next check in: ${formatEventRemaining(secsUntil(nextCheckAt))}`;
	} else if (phase === "ended") {
		countdownText = "Finished";
	}

	return (
		<section className="rounded border px-3 py-2">
			{/* Row 1：阶段控制（SegmentedControl）| End —— space-between */}
			<div className="flex items-center justify-between gap-4">
				{controllable ? (
					<div className="flex items-center gap-2 min-w-0">
						<SegmentedControl
							size="small"
							aria-label="AWDP 阶段控制"
							onChange={(index) => {
								const target = index === 0 ? "break" : "fix";
								if (target !== phase) {
									onSetPhase(target);
								}
							}}
						>
							<SegmentedControl.Button
								selected={phase === "break"}
								disabled={phaseBusy}
								leadingIcon={BugIcon}
							>
								Break
							</SegmentedControl.Button>
							<SegmentedControl.Button
								selected={phase === "fix"}
								disabled={phaseBusy}
								leadingIcon={ToolsIcon}
							>
								Fix
							</SegmentedControl.Button>
						</SegmentedControl>
						{phase === "fix" && (
							<span className="text-xs font-semibold uppercase tracking-wide text-[var(--fgColor-muted)] tabular-nums">
								Turn {currentRound} / {totalRounds}
							</span>
						)}
					</div>
				) : (
					<div className="flex items-center gap-2 flex-wrap min-w-0">
						<Label variant={meta.variant}>{meta.text}</Label>
						{phase === "fix" && (
							<span className="text-xs font-semibold uppercase tracking-wide text-[var(--fgColor-muted)] tabular-nums">
								Turn {currentRound} / {totalRounds}
							</span>
						)}
						{!practice && (
							<p className="text-xs text-[var(--fgColor-muted)]">
								{meta.description}
							</p>
						)}
					</div>
				)}
				{onEnd && (phase === "break" || phase === "fix") ? (
					<Button
						variant="danger"
						size="small"
						disabled={endBusy}
						onClick={() => onEnd()}
					>
						{endRunning ? "停止中…" : "End"}
					</Button>
				) : null}
			</div>

			{/* Row 2：事件式倒计时 | Score（压缩单行） */}
			<div className="mt-1.5 flex items-baseline justify-between gap-4">
				<p className="min-w-0 text-lg leading-5 font-semibold tabular-nums text-[var(--fgColor-default)]">
					{countdownText}
				</p>
				<p className="shrink-0 text-lg leading-5 font-semibold tabular-nums text-[var(--fgColor-default)]">
					{formatScore(score)}
					<span className="ml-1.5 text-[11px] font-semibold uppercase tracking-wide text-[var(--fgColor-muted)]">
						{phase === "ended" ? "Final Score" : "Score"}
					</span>
				</p>
			</div>

			{/* Row 3：Break → Fix Timeline */}
			<div className="mt-1.5">
				<AwdpTimeline state={state} phase={phase} totalRounds={totalRounds} />
			</div>
		</section>
	);
}
