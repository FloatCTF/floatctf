import type { FloatCTFClient } from "@floatctf/sdk";
import { queryOptions } from "@tanstack/react-query";

/**
 * 题目详情 query options 工厂。
 * Key: ["challenge", id] — 必须与既有 key 完全一致。
 */
export function challengeQueryOptions(client: FloatCTFClient, id: string) {
	return queryOptions({
		queryKey: ["challenge", id],
		queryFn: () => client.service.challenges.get(id),
	});
}

/**
 * 题目实例 query options 工厂。
 * Key: ["instance", id] — 必须与既有 key 完全一致。
 *
 * 注意：非动态题 instance 可能 404。loader 中用尽力预取
 * （.catch()），不要硬 ensureQueryData。
 */
export function challengeInstanceQueryOptions(client: FloatCTFClient, id: string) {
	return queryOptions({
		queryKey: ["instance", id],
		queryFn: () => client.service.challenges.getInstance(id),
	});
}
