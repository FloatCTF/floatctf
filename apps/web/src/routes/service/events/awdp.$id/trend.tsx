import { Spinner } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import type { AxiosError } from "axios";

import { type AwdpTrendItem, awdpPlayerApi } from "@/api/awdp";
import type { UniResponse } from "@/api/axios";
import { TrendChart } from "@/routes/service/events/jeopardy.$id/trend";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awdp/$id/trend")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const { data, isLoading, isError, error } = useQuery<
		UniResponse<AwdpTrendItem[]>,
		AxiosError<{ message: string }>
	>({
		queryKey: ["awdp-trend", id],
		queryFn: () => awdpPlayerApi.trend(id),
		refetchInterval: 30000, // 30 秒自动刷新（回合推进分数变化）
	});

	if (isLoading) {
		return <Spinner size="large" />;
	}
	if (isError) {
		return <div>{error.response?.data.message ?? error.message}</div>;
	}

	return (
		<div className="w-full h-full flex">
			{data?.data && (
				<TrendChart
					className="flex justify-center items-center"
					data={data.data}
				/>
			)}
		</div>
	);
}
