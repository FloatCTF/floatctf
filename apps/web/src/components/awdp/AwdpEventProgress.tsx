import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

import { serviceApi } from "@/api";
import { type AwdpOverview, awdpPlayerApi } from "@/api/awdp";
import {
	AwdpTimeline,
	computeTimelineState,
	formatEventRemaining,
} from "@/components/awdp/AwdpPhaseOverview";

/**
 * 当前阶段的 deadline（本地时刻）；到点后需要重新获取数据（阶段/回合切换）。
 * pending → start_time；break → break_ends_at；fix → 下一轮 cutoff 与 fix 结束
 * 取较早者（round 推进与阶段结束都要求刷新）；其余阶段无 deadline。
 */
function stageDeadlineMs(
	o: AwdpOverview | undefined,
	startTime?: string | null,
): number | null {
	if (!o) {
		return null;
	}
	switch (o.phase) {
		case "pending":
			return startTime ? Date.parse(startTime) : null;
		case "break":
			return o.break_ends_at ? Date.parse(o.break_ends_at) : null;
		case "fix": {
			const ts = [o.next_action_at, o.fix_ends_at]
				.filter((x): x is string => Boolean(x))
				.map((x) => Date.parse(x))
				.filter(Number.isFinite);
			return ts.length ? Math.min(...ts) : null;
		}
		default:
			return null;
	}
}

/**
 * AWDP 赛事顶部进度条（route.tsx 标题与导航之间，位置对齐 Jeopardy RemainingTimer）。
 *
 * 与 Jeopardy 的 start→end 单一进度不同，AWDP 进度条按真实时长比例分段：
 *   - Break 段（accent）→ Fix 段（success），段间用竖线分隔（分段的那个竖线）；
 *   - Fix 段内按 totalRounds 均分 Turn 分隔竖线；
 *   - marker 由真实 started_at / fix_started_at 计算，elapsed 填充带动画条纹。
 *
 * 数据源：awdp-overview（participant 级，未加入赛事时 403 → 不渲染；Join 后出现）。
 * 倒计时到点：本地 now 越过当前阶段 deadline 时**立即 refetch**，并以 2s 间隔
 * 轮询兜底，直到 overview 反映新阶段（tick 10s 推进 + SSE 推送之间不依赖 poll）。
 */
export function AwdpEventProgress({ id }: { id: string }) {
	const [now, setNow] = useState(() => Date.now());
	const deadlineHitRef = useRef(false);

	useEffect(() => {
		const timer = setInterval(() => setNow(Date.now()), 1000);
		return () => clearInterval(timer);
	}, []);

	const { data, refetch } = useQuery({
		queryKey: ["awdp-overview", id],
		queryFn: () => awdpPlayerApi.overview(id),
		retry: false,
	});
	const overview = data?.data;

	// pending 需要事件 start_time 计算「还有多久开始」（awdp-overview 无 start_time）。
	const { data: eventData } = useQuery({
		queryKey: ["eventInfo", id],
		queryFn: () => serviceApi.events.get(id),
		retry: false,
	});
	const ev = eventData?.data?.event;

	// 倒计时到点：越过 deadline 的瞬间立即 refetch 一次（用户要求此刻重新获取）。
	// 随后以 2s 兜底轮询，直到 overview 反映新阶段（refetch 后 phase/deadline 变化
	// → 不再到点 → 自动停止），不依赖 SSE/15s poll 的延迟。
	useEffect(() => {
		const d = stageDeadlineMs(overview, ev?.start_time);
		if (d !== null && now >= d) {
			if (!deadlineHitRef.current) {
				deadlineHitRef.current = true;
				void refetch();
			}
		} else if (deadlineHitRef.current) {
			// 阶段已变化/未到点 → 复位，等下一个 deadline。
			deadlineHitRef.current = false;
		}
	}, [now, overview, ev?.start_time, refetch]);

	// 供兜底 interval 读取最新快照（避免闭包捕获过期值）。
	const overviewRef = useRef<AwdpOverview | undefined>(undefined);
	const startTimeRef = useRef<string | null | undefined>(undefined);
	overviewRef.current = overview;
	startTimeRef.current = ev?.start_time;

	// 兜底：到点期间每 2s 轮询 refetch（阶段切换 + 后端 tick 之间不留 15s 窗口）。
	useEffect(() => {
		const timer = setInterval(() => {
			const d = stageDeadlineMs(overviewRef.current, startTimeRef.current);
			if (d !== null && Date.now() >= d) {
				void refetch();
			}
		}, 2000);
		return () => clearInterval(timer);
	}, [refetch]);

	if (!overview) {
		return null;
	}

	const state = computeTimelineState({
		phase: overview.phase,
		breakDurationSecs: overview.break_duration_secs,
		fixDurationSecs: overview.fix_duration_secs,
		totalRounds: overview.total_rounds,
		startedAt: overview.started_at,
		fixStartedAt: overview.fix_started_at,
		now,
	});

	const secsUntil = (iso?: string | null) => {
		if (!iso) return null;
		const t = Date.parse(iso);
		if (!Number.isFinite(t)) return null;
		return Math.max(0, (t - now) / 1000);
	};

	// 阶段名 + 剩余时间（Break xxxxxxx / Fix xxxxxxx 风格；
	// pending 显示还有多久开始；preparing_fix 过渡提示；ended 显示 Finished）。
	let countdownText = "Waiting to start";
	if (overview.phase === "break") {
		countdownText = `Break ${formatEventRemaining(secsUntil(overview.break_ends_at))}`;
	} else if (overview.phase === "fix") {
		countdownText = `Fix ${formatEventRemaining(secsUntil(overview.next_action_at))}`;
	} else if (overview.phase === "pending") {
		countdownText = `Starts in: ${formatEventRemaining(secsUntil(ev?.start_time))}`;
	} else if (overview.phase === "preparing_fix") {
		countdownText = "Preparing Fix…";
	} else if (overview.phase === "ended") {
		countdownText = "Finished";
	}

	return (
		<div className="mt-2">
			<div className="flex items-center justify-between text-sm tabular-nums text-[var(--fgColor-muted)]">
				<span>{countdownText}</span>
				{overview.phase === "fix" && (
					<span className="text-xs font-semibold uppercase tracking-wide">
						Turn {overview.current_round} / {overview.total_rounds}
					</span>
				)}
			</div>
			<AwdpTimeline
				state={state}
				phase={overview.phase}
				totalRounds={overview.total_rounds}
			/>
		</div>
	);
}
