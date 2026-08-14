import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";

import { serviceApi } from "@/api";
import { awdpPlayerApi } from "@/api/awdp";
import {
	AwdpTimeline,
	computeTimelineState,
	formatEventRemaining,
} from "@/components/awdp/AwdpPhaseOverview";

/**
 * AWDP 赛事顶部进度条（route.tsx 标题与导航之间，位置对齐 Jeopardy RemainingTimer）。
 *
 * 与 Jeopardy 的 start→end 单一进度不同，AWDP 进度条按真实时长比例分段：
 *   - Break 段（accent）→ Fix 段（success），段间用竖线分隔（分段的那个竖线）；
 *   - Fix 段内按 totalRounds 均分 Turn 分隔竖线；
 *   - marker 由真实 started_at / fix_started_at 计算，elapsed 填充带动画条纹。
 *
 * 数据源：awdp-overview（participant 级，未加入赛事时 403 → 不渲染；Join 后出现）。
 */
export function AwdpEventProgress({ id }: { id: string }) {
	const [now, setNow] = useState(() => Date.now());

	useEffect(() => {
		const timer = setInterval(() => setNow(Date.now()), 1000);
		return () => clearInterval(timer);
	}, []);

	const { data } = useQuery({
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
	// pending 显示还有多久开始；ended 显示 Finished）。
	let countdownText = "Waiting to start";
	if (overview.phase === "break") {
		countdownText = `Break ${formatEventRemaining(secsUntil(overview.break_ends_at))}`;
	} else if (overview.phase === "fix") {
		countdownText = `Fix ${formatEventRemaining(secsUntil(overview.next_action_at))}`;
	} else if (overview.phase === "pending") {
		countdownText = `Starts in: ${formatEventRemaining(secsUntil(ev?.start_time))}`;
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
