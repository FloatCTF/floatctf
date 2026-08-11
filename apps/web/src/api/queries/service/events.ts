import { serviceApi } from "@/api";
import { queryOptions } from "@tanstack/react-query";

/**
 * 赛事信息 query options 工厂。
 * 用于路由 loader（ensureQueryData）与组件（useQuery）。
 *
 * Key: ["eventInfo", id] — 必须与既有 key 完全一致。
 */
export const eventInfoQueryOptions = (id: string) =>
	queryOptions({
		queryKey: ["eventInfo", id],
		queryFn: () => serviceApi.events.get(id),
	});
