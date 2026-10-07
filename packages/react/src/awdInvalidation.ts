import type { QueryClient } from "@tanstack/react-query";

/**
 * AWD 赛事页用到的 query key —— 事件流失效与页面手动刷新**共用同一份常量**。
 *
 * 背景：SSE 连上后轮询回退会停止（两者互斥，刻意设计）。因此事件流的失效列表一旦比
 * 页面实际读取的 key 窄，面板就会在整场比赛中只靠手动刷新更新 —— 而 websocket 看起来
 * 是「正常」的（分数确实在动），很容易被漏掉。抽成常量就是为了不再漂移：
 * 新增 AWD 页面 query key 时改这里，hook 与手动刷新同时生效。
 */

/** 选手端 AWD 赛事页（/service/events/awd/{id}/*）读取的 key。 */
export const AWD_PLAYER_QUERY_KEYS = [
	"eventInfo",
	"event",
	"awd-gameboxes",
	"awd-scores",
	"awd-wg",
	"awd-ssh",
	"awd-player-status",
	"announcements",
] as const;

/** 管理端 AWD 赛事页（/admin/events/awd/{id}/*）读取的 key。 */
export const AWD_ADMIN_QUERY_KEYS = [
	"event",
	"eventInfo",
	"awd-gameboxes",
	"awd-scores",
	"admin-awd-scores",
	"admin-awd-status",
	"admin-awd-prechecks",
] as const;

/** 按 `[key, eventId]` 失效整组 key（与各页面 `useQuery({ queryKey: [key, id] })` 对齐）。 */
export function invalidateAwdQueries(
	client: QueryClient,
	eventId: string,
	keys: readonly string[],
): void {
	for (const key of keys) {
		void client.invalidateQueries({ queryKey: [key, eventId] });
	}
}
