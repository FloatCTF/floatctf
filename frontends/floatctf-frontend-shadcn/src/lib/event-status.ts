/**
 * 赛事语义 —— 状态判定与文案。
 *
 * 状态由 `start_time` / `end_time` 推导，**与后端守卫同一套规则**（后端的 `time_status()`
 * 三态：not_started / ongoing / ended；练习赛事 `end_time` 为空时永不结束）。这里只做展示，
 * 不替代后端判定：任何写操作仍由后端拒绝越界的请求。
 */

import type { Events } from "@floatctf/sdk/entity";

export type EventStatus = "upcoming" | "ongoing" | "ended" | "unknown";

export type StatusTone = "success" | "info" | "muted" | "warning" | "danger";

export function computeEventStatus(
	startTime: string | null | undefined,
	endTime: string | null | undefined,
	nowMs: number = Date.now(),
): EventStatus {
	if (!startTime) return "unknown";
	const start = new Date(startTime).getTime();
	if (Number.isNaN(start)) return "unknown";
	if (start > nowMs) return "upcoming";
	if (endTime === null || endTime === undefined || endTime === "") {
		// 练习 / 开放式赛事：不因墙钟判定结束。
		return "ongoing";
	}
	const end = new Date(endTime).getTime();
	if (Number.isNaN(end)) return "unknown";
	return end < nowMs ? "ended" : "ongoing";
}

export function eventStatusOf(event: Events, nowMs?: number): EventStatus {
	return computeEventStatus(event.start_time, event.end_time ?? null, nowMs);
}

export const EVENT_STATUS_LABEL: Record<EventStatus, string> = {
	upcoming: "未开始",
	ongoing: "进行中",
	ended: "已结束",
	unknown: "时间未知",
};

export const EVENT_STATUS_TONE: Record<EventStatus, StatusTone> = {
	upcoming: "info",
	ongoing: "success",
	ended: "muted",
	unknown: "warning",
};

export const EVENT_FAMILY_LABEL: Record<string, string> = {
	jeopardy: "Jeopardy 解题",
	awd: "AWD 攻防",
	awdp: "AWDP 攻防",
};

export const PARTICIPANT_MODE_LABEL: Record<string, string> = {
	individual: "个人赛",
	team: "战队赛",
};

export const EVENT_PURPOSE_LABEL: Record<string, string> = {
	competition: "正式赛",
	practice: "练习",
};

export interface EventCapabilities {
	/** 是否有题目集合（Jeopardy / AWDP）。 */
	challenges: boolean;
	/** 是否有靶机实例（Jeopardy 动态题 / AWDP）。 */
	instances: boolean;
	/** 是否有 AWD 驾驶舱。 */
	arena: boolean;
	/** 是否有 AWDP 工作台。 */
	lab: boolean;
	/** 是否有解题积分榜。 */
	scoreboard: boolean;
	/** 是否有分数趋势。 */
	trend: boolean;
	/** 是否有 WireGuard 配置下发。 */
	wireguard: boolean;
	/** 是否有 GameBox 列表。 */
	gameboxes: boolean;
	/** 是否有轮次 / 评测。 */
	rounds: boolean;
	/** 是否有修正包上传与自检。 */
	judge: boolean;
	/** 是否允许重置实例。 */
	reset: boolean;
	/** 是否支持战队。 */
	teams: boolean;
}

/**
 * 由赛事本身推导能力位（与后端 `EventCapabilities::for_mode` 同构）。
 * 不使用 `GET /events/{id}/capabilities` 逃生舱：`family` / `participant_mode` 已在
 * `EventInfo.event` 里，语义完全一致。
 */
export function eventCapabilities(event: Events): EventCapabilities {
	const team = event.participant_mode === "team";
	if (event.family === "awd") {
		return {
			challenges: false,
			instances: false,
			arena: true,
			lab: false,
			scoreboard: true,
			trend: true,
			wireguard: true,
			gameboxes: true,
			rounds: true,
			judge: false,
			reset: true,
			teams: team,
		};
	}
	if (event.family === "awdp") {
		return {
			challenges: true,
			instances: true,
			arena: false,
			lab: true,
			scoreboard: true,
			trend: true,
			wireguard: false,
			gameboxes: true,
			rounds: true,
			judge: true,
			reset: true,
			teams: team,
		};
	}
	return {
		challenges: true,
		instances: true,
		arena: false,
		lab: false,
		scoreboard: true,
		trend: true,
		wireguard: false,
		gameboxes: false,
		rounds: false,
		judge: false,
		reset: true,
		teams: team,
	};
}

/**
 * 取赛事 ID。
 *
 * **已核实的后端事实**：选手端 `GET /events` 与 `GET /events/{id}` 返回的 `EventInfo`
 * **不包含顶层 `id`**（SDK 类型声明里有，但运行时为 `undefined`），真值只在
 * `info.event.id`。因此本前端的赛事相关代码一律用本函数取 ID，绝不直接用 `info.id`
 * （否则会请求 `/events/undefined/...`）。
 */
export function eventIdOf(info: { id?: string | null; event: Events }): string {
	const fromEvent = info.event?.id;
	if (typeof fromEvent === "string" && fromEvent.length > 0) return fromEvent;
	return typeof info.id === "string" ? info.id : "";
}

export function isPracticeEvent(event: Events): boolean {
	return event.purpose === "practice" || event.is_virtual;
}
