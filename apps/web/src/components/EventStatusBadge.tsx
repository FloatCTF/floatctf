import { Label, type LabelProps } from "@primer/react";

export type EventStatus = "upcoming" | "ongoing" | "ended" | "unknown";

/**
 * 依据赛事 start_time / end_time 计算当前比赛状态。
 * Practice（end_time 为空）：start 前 upcoming，之后 ongoing（无 ended）。
 * Competition：标准 start/end 窗口。
 */
export function computeEventStatus(
	startTime: string,
	endTime?: string | null,
	nowMs: number = Date.now(),
): EventStatus {
	const start = new Date(startTime).getTime();
	if (Number.isNaN(start)) return "unknown";
	if (start > nowMs) return "upcoming";
	if (endTime == null || endTime === "") {
		// 练习/开放式：不因墙钟判定结束
		return "ongoing";
	}
	const end = new Date(endTime).getTime();
	if (Number.isNaN(end)) return "unknown";
	if (end < nowMs) return "ended";
	return "ongoing";
}

/** 与 Logs 页 Level 徽章同款：Primer Label + variant，纯文字无圆点 */
const STATUS_VARIANT: Record<
	EventStatus,
	NonNullable<LabelProps["variant"]>
> = {
	ongoing: "success",
	upcoming: "accent",
	ended: "default",
	unknown: "default",
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
}: {
	startTime: string;
	endTime?: string | null;
}) {
	const status = computeEventStatus(startTime, endTime);
	return (
		<Label variant={STATUS_VARIANT[status]}>{EVENT_STATUS_LABEL[status]}</Label>
	);
}
