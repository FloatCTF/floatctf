/**
 * 赛事语义的**唯一**出处。
 *
 * 两个刻意的约束：
 *
 * 1. **`EventInfo.id` 运行时不返回** —— 后端只保证 `event.id`。
 *    因此全仓一律用 `eventIdOf(info)`，不要在页面里写 `info.id`
 *    （那样 TS 不报错、运行时是 undefined，会把 URL 拼成 `/events/undefined`）。
 * 2. 状态**由真实字段推导**，不发明枚举：后端给的是 `start_time` / `end_time` /
 *    `hidden`，没有"status"字段，因此这里只做时间比较，语义与后端一致。
 */
import type { EventInfo } from "@floatctf/sdk";
import { EventFamily, EventPurpose, ParticipantMode } from "@floatctf/sdk/entity";

export function eventIdOf(info: EventInfo): string {
	return info.event.id;
}

export type EventStatus = "upcoming" | "running" | "ended";

export function eventStatus(info: EventInfo, now = new Date()): EventStatus {
	const start = new Date(info.event.start_time);
	const end = info.event.end_time ? new Date(info.event.end_time) : null;
	if (now < start) return "upcoming";
	if (end && now > end) return "ended";
	return "running";
}

/** 状态 → 展示文案 + 调色板颜色（全站统一，不按页面随机换色）。 */
export const STATUS_META: Record<
	EventStatus,
	{ label: string; color: "app-teal" | "app-yellow" | "brown" }
> = {
	running: { label: "进行中", color: "app-teal" },
	upcoming: { label: "未开始", color: "app-yellow" },
	ended: { label: "已结束", color: "brown" },
};

export const FAMILY_LABEL: Record<EventFamily, string> = {
	[EventFamily.Jeopardy]: "Jeopardy",
	[EventFamily.Awd]: "AWD",
	[EventFamily.Awdp]: "AWDP",
};

/** 赛制 → 卡片色调（卡片用颜色表达赛制，列表扫一眼就能分辨）。 */
export const FAMILY_COLOR: Record<
	EventFamily,
	"app-blue" | "app-orange" | "app-green"
> = {
	[EventFamily.Jeopardy]: "app-blue",
	[EventFamily.Awd]: "app-orange",
	[EventFamily.Awdp]: "app-green",
};

export const PURPOSE_LABEL: Record<EventPurpose, string> = {
	[EventPurpose.Practice]: "练习",
	[EventPurpose.Competition]: "正式比赛",
};

export const MODE_LABEL: Record<ParticipantMode, string> = {
	[ParticipantMode.Individual]: "个人",
	[ParticipantMode.Team]: "团队",
};

/** 统一的时间展示（本地时区，精确到分钟）。 */
export function formatTime(iso: string | undefined | null): string {
	if (!iso) return "—";
	const d = new Date(iso);
	if (Number.isNaN(d.getTime())) return "—";
	return d.toLocaleString("zh-CN", {
		year: "numeric",
		month: "2-digit",
		day: "2-digit",
		hour: "2-digit",
		minute: "2-digit",
	});
}

/** 相对时间：赛事列表里"还有多久开始"比绝对时间更有用。 */
export function relativeTime(iso: string, now = new Date()): string {
	const target = new Date(iso).getTime();
	const diff = target - now.getTime();
	const abs = Math.abs(diff);
	const mins = Math.round(abs / 60000);
	const hours = Math.round(abs / 3600000);
	const days = Math.round(abs / 86400000);
	const unit = days >= 1 ? `${days} 天` : hours >= 1 ? `${hours} 小时` : `${mins} 分钟`;
	return diff >= 0 ? `${unit}后` : `${unit}前`;
}
