import { adminApi } from "@/api";
import { queryOptions } from "@tanstack/react-query";

/**
 * Query options factory for admin system information.
 * Key: ["system_information"] — must match existing key exactly.
 *
 * Note: the admin dashboard also uses refetchInterval: 60s.
 * That stays in the component — loader only preloads initial data.
 */
export const systemInformationQueryOptions = () =>
	queryOptions({
		queryKey: ["system_information"],
		queryFn: adminApi.system.monitor,
	});
