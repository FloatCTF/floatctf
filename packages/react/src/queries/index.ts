import type { FloatCTFClient } from "@floatctf/sdk";

import { challengeInstanceQueryOptions, challengeQueryOptions } from "./challenge.js";
import { eventInfoQueryOptions } from "./eventInfo.js";
import { systemInformationQueryOptions } from "./systemInformation.js";

/**
 * 把 query options 工厂绑定到某个 FloatCTF 客户端。
 *
 * 返回的工厂保持与原 `@/api/queries` **完全相同的调用签名与 query key**，
 * 因此迁移到 headless 绑定后页面的 invalidate/refetch 语义零变化。
 */
export function createQueryFactories(client: FloatCTFClient) {
	return {
		eventInfoQueryOptions: (id: string) => eventInfoQueryOptions(client, id),
		challengeQueryOptions: (id: string) => challengeQueryOptions(client, id),
		challengeInstanceQueryOptions: (id: string) =>
			challengeInstanceQueryOptions(client, id),
		systemInformationQueryOptions: () => systemInformationQueryOptions(client),
	};
}

export { challengeInstanceQueryOptions, challengeQueryOptions } from "./challenge.js";
export { eventInfoQueryOptions } from "./eventInfo.js";
export { systemInformationQueryOptions } from "./systemInformation.js";
