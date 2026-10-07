import type { FloatCTFClient } from "@floatctf/sdk";
import { queryOptions } from "@tanstack/react-query";

/**
 * 管理端系统信息 query options 工厂。
 * Key: ["system_information"] — 必须与既有 key 完全一致。
 *
 * 注意：管理端仪表盘另有 refetchInterval: 60s。
 * 该逻辑留在组件——loader 只预加载首屏数据。
 */
export function systemInformationQueryOptions(client: FloatCTFClient) {
	return queryOptions({
		queryKey: ["system_information"],
		queryFn: () => client.admin.system.monitor(),
	});
}
