import { serviceApi } from "@/api";
import { queryOptions } from "@tanstack/react-query";

/**
 * Query options factory for challenge detail.
 * Key: ["challenge", id] — must match existing key exactly.
 */
export const challengeQueryOptions = (id: string) =>
	queryOptions({
		queryKey: ["challenge", id],
		queryFn: () => serviceApi.challenges.get(id),
	});

/**
 * Query options factory for challenge instance.
 * Key: ["instance", id] — must match existing key exactly.
 *
 * Note: instance may 404 for non-dynamic challenges. Use best-effort
 * prefetch (with .catch()) in loaders, NOT hard ensureQueryData.
 */
export const challengeInstanceQueryOptions = (id: string) =>
	queryOptions({
		queryKey: ["instance", id],
		queryFn: () => serviceApi.challenges.getInstance(id),
	});
