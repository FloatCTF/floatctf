/**
 * AWD 运维页的数据读取 hooks —— 页面与各标签共用同一份 queryKey（React Query 自动去重），
 * key 一律取自 `qk`，其中 `qk.awd.adminStatus / adminScores / prechecks / eventGameboxes /
 * eventNetwork` 与 `@floatctf/react` 的 `AWD_ADMIN_QUERY_KEYS` 对齐，SSE 到达时能失效到本页。
 *
 * 两个已核实的后端语义（CONVENTIONS §6.11 / §6.12）：
 * - `getStatus` 在赛事尚未开通 AWD 时返回 `data = null`（不是错误）→ 用 `callMaybe`；
 * - `getEventNetwork` 在**未分配**时返回 404（reject）→ 映射成「未分配」而不是加载失败。
 */

import type {
	AwdEventStatus,
	EventGameBoxDto,
	EventNetworkInfo,
	GameBoxLibraryDto,
	TeamResult,
} from "@floatctf/sdk";
import type { Events } from "@floatctf/sdk/entity";
import { useQuery } from "@tanstack/react-query";

import { call, callList, callMaybe } from "~/api/call";
import { useClient } from "~/api/client";
import { isNotFound } from "~/api/errors";
import { qk } from "~/api/keys";

/** 列表单次拉取上限（服务端分页；key 里不带参数，避免与 SSE 的 `[key, eventId]` 失效失配）。 */
export const AWD_LIST_LIMIT = 100;

/** 赛事本体（标题 / 赛制 / 时间窗）—— 只用公共 SDK 读，不依赖赛事管理 feature。 */
export function useAwdEvent(eventId: string) {
	const client = useClient();
	return useQuery({
		queryKey: qk.admin.event(eventId),
		queryFn: () => call<Events>(client.admin.events.get(eventId), "赛事详情"),
		enabled: eventId !== "",
	});
}

/** AWD 赛事状态；`null` = 该赛事尚未开通 AWD（不是错误）。 */
export function useAwdStatus(eventId: string) {
	const client = useClient();
	return useQuery({
		queryKey: qk.awd.adminStatus(eventId),
		queryFn: () => callMaybe<AwdEventStatus>(client.awd.admin.getStatus(eventId), "AWD 赛事状态"),
		enabled: eventId !== "",
	});
}

/** 赛事网络；`null` = 尚未分配（后端 404）。 */
export function useAwdEventNetwork(eventId: string) {
	const client = useClient();
	return useQuery({
		queryKey: qk.awd.eventNetwork(eventId),
		queryFn: async (): Promise<EventNetworkInfo | null> => {
			try {
				return await callMaybe<EventNetworkInfo>(
					client.awd.admin.getEventNetwork(eventId),
					"赛事网络",
				);
			} catch (error) {
				// 未分配时后端 404：这是「未分配」业务状态，不是加载失败。
				if (isNotFound(error)) return null;
				throw error;
			}
		},
		enabled: eventId !== "",
	});
}

/** 参赛战队（封禁状态来自 `event_teams.banned`，与 AWD 封禁服务同源）。 */
export function useAwdTeams(eventId: string) {
	const client = useClient();
	return useQuery({
		queryKey: qk.admin.eventTeams(eventId),
		// 柯里化方法：getTeams(eventId) 返回的才是请求函数（第二层无参数）。
		queryFn: () => call<TeamResult[]>(client.admin.event_teams.getTeams(eventId)(), "战队列表"),
		enabled: eventId !== "",
	});
}

/** 赛事已挂载的 GameBox（EventGameBox）。 */
export function useAwdEventGameboxes(eventId: string) {
	const client = useClient();
	return useQuery({
		queryKey: qk.awd.eventGameboxes(eventId),
		queryFn: async () => {
			const { items, meta } = await callList<EventGameBoxDto>(
				client.awd.admin.listEventGameboxes(eventId, { limit: AWD_LIST_LIMIT }),
			);
			return { items, total: meta.total ?? items.length };
		},
		enabled: eventId !== "",
	});
}

/** GameBox 库（选择要挂到赛事的靶机）。 */
export function useAwdGameboxLibrary(enabled: boolean) {
	const client = useClient();
	const params = { limit: AWD_LIST_LIMIT };
	return useQuery({
		queryKey: qk.awd.library(params),
		queryFn: async () => {
			const { items, meta } = await callList<GameBoxLibraryDto>(
				client.awd.admin.listGameboxes(params),
			);
			return { items, total: meta.total ?? items.length };
		},
		enabled,
	});
}
