import type { FloatCTFClient } from "@floatctf/sdk";
import { queryOptions } from "@tanstack/react-query";

/**
 * 赛事信息 query options 工厂。
 * 用于路由 loader（ensureQueryData）与组件（useQuery）。
 *
 * Key: ["eventInfo", id] — 必须与既有 key 完全一致。
 *
 * 注意：这里不设 staleTime: 0。路由 loader 用 ensureQueryData，
 * staleTime: 0 会让数据永远 stale → 每次从列表进详情都重新请求并等待，
 * 造成白屏。用户状态刷新（join/leave/建队/退队）由 mutation 的
 * invalidateQueries(["eventInfo", id]) 保证（active 查询立即 refetch）。
 */
export function eventInfoQueryOptions(client: FloatCTFClient, id: string) {
	return queryOptions({
		queryKey: ["eventInfo", id],
		queryFn: () => client.service.events.get(id),
	});
}
