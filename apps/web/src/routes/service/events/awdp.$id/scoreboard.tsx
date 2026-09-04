import type { UniResponse } from "@/api/axios";
import { Spinner } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import type { AxiosError } from "axios";

import { type AwdpScoreboardDetail, awdpPlayerApi } from "@/api/awdp";
import { AwdpScoreboardView } from "@/components/awdp/AwdpScoreboard";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awdp/$id/scoreboard")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

const EMPTY: AwdpScoreboardDetail = {
	participant_mode: "individual",
	gameboxes: [],
	rounds: [],
	rows: [],
};

function RouteComponent() {
	const { id } = Route.useParams();
	const { data, isLoading, isError, error } = useQuery<
		UniResponse<AwdpScoreboardDetail>,
		AxiosError<{ message: string }>
	>({
		queryKey: ["awdp-scoreboard", id],
		queryFn: () => awdpPlayerApi.scoreboard(id),
		refetchInterval: 30000, // 30 秒自动刷新（回合推进分数变化）
	});

	if (isLoading) {
		return <Spinner size="large" />;
	}
	if (isError) {
		return <div>{error.response?.data.message ?? error.message}</div>;
	}

	return <AwdpScoreboardView data={data?.data ?? EMPTY} className="mt-2" />;
}
