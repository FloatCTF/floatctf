import { Label, type LabelProps } from "@primer/react";

export type EventStatus = "upcoming" | "ongoing" | "ended" | "unknown";

/**
 * 依据赛事 start_time / end_time 计算当前比赛状态（单一数据源）。
 * 语义与 routes/service/events/jeopardy.$id/index.tsx 的 getEventStatus 对齐：
 * upcoming（未开始）/ ongoing（进行中）/ ended（已结束）/ unknown（时间字段缺失或非法）。
 */
export function computeEventStatus(
	startTime: string,
	endTime: string,
	nowMs: number = Date.now(),
): EventStatus {
	const start = new Date(startTime).getTime();
	const end = new Date(endTime).getTime();
	if (Number.isNaN(start) || Number.isNaN(end)) return "unknown";
	if (start > nowMs) return "upcoming";
	if (end < nowMs) return "ended";
	return "ongoing";
}

/** 与 Logs 页 Level 徽章同款：Primer Label + variant */
const STATUS_VARIANT: Record<
	EventStatus,
	NonNullable<LabelProps["variant"]>
> = {
	ongoing: "success",
	upcoming: "accent",
	ended: "default",
	unknown: "default",
};

const STATUS_DOT: Record<EventStatus, string> = {
	ongoing: "bg-[var(--fgColor-success)]",
	upcoming: "bg-[var(--fgColor-accent)]",
	ended: "bg-[var(--fgColor-muted)]",
	unknown: "bg-[var(--fgColor-muted)]",
};

export const EVENT_STATUS_LABEL: Record<EventStatus, string> = {
	ongoing: "Ongoing",
	upcoming: "Upcoming",
	ended: "Ended",
	unknown: "TBD",
};

export function EventStatusBadge({
	startTime,
	endTime,
	showDot = true,
}: {
	startTime: string;
	endTime: string;
	showDot?: boolean;
}) {
	const status = computeEventStatus(startTime, endTime);
	return (
		<Label variant={STATUS_VARIANT[status]}>
			{showDot && (
				<span
					className={`w-2 h-2 rounded-full inline-block mr-1 ${STATUS_DOT[status]}`}
				/>
			)}
			{EVENT_STATUS_LABEL[status]}
		</Label>
	);
}
