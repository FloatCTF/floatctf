export type EventStatus = "live" | "upcoming" | "ended";

/**
 * 依据赛事 start_time / end_time 计算当前比赛状态（单一数据源）。
 * 与数据库时间无关的「隐藏/允许加入」等字段不参与计算。
 */
export function computeEventStatus(
	startTime: string,
	endTime: string,
): EventStatus {
	const now = Date.now();
	const start = new Date(startTime).getTime();
	const end = new Date(endTime).getTime();
	// 时间字段缺失/非法时按已结束处理，绝不误报为进行中
	if (Number.isNaN(start) || Number.isNaN(end)) return "ended";
	if (now < start) return "upcoming";
	if (now <= end) return "live";
	return "ended";
}

const STATUS_STYLE: Record<EventStatus, string> = {
	live: "bg-[var(--bgColor-success)] text-[var(--fgColor-success)]",
	upcoming: "bg-[var(--bgColor-accent)] text-[var(--fgColor-accent)]",
	ended: "bg-[var(--bgColor-muted)] text-[var(--fgColor-muted)]",
};

const STATUS_DOT: Record<EventStatus, string> = {
	live: "bg-[var(--fgColor-success)]",
	upcoming: "bg-[var(--fgColor-accent)]",
	ended: "bg-[var(--fgColor-muted)]",
};

export const EVENT_STATUS_LABEL: Record<EventStatus, string> = {
	live: "进行中",
	upcoming: "未开始",
	ended: "已结束",
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
		<span
			className={`inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-xs font-medium whitespace-nowrap ${STATUS_STYLE[status]}`}
		>
			{showDot && (
				<span
					className={`w-2 h-2 rounded-full flex-shrink-0 ${STATUS_DOT[status]}`}
				/>
			)}
			{EVENT_STATUS_LABEL[status]}
		</span>
	);
}
