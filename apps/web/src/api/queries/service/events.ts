import { serviceApi } from "@/api";
import { queryOptions } from "@tanstack/react-query";

/**
 * Query options factory for event info.
 * Used in route loaders (ensureQueryData) and components (useQuery).
 *
 * Key: ["eventInfo", id] — must match existing key exactly.
 */
export const eventInfoQueryOptions = (id: string) =>
	queryOptions({
		queryKey: ["eventInfo", id],
		queryFn: () => serviceApi.events.get(id),
	});
