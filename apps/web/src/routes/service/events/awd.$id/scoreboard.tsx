import type { UniResponse } from "@/api/axios";
import { Spinner } from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";
import type { AxiosError } from "axios";

import { serviceApi } from "@/api";
import type { AwdScoreRow } from "@/api/awd";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awd/$id/scoreboard")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();

	// Get team info to highlight current player's team
	const { data: eventData } = useQuery({
		queryKey: ["eventInfo", id],
		queryFn: () => serviceApi.events.get(id),
	});

	const { data, isLoading, isError, error } = useQuery<
		UniResponse<AwdScoreRow[]>,
		AxiosError<{ message: string }>
	>({
		queryKey: ["awd-scores", id],
		queryFn: () => serviceApi.awd.scores(id),
		refetchInterval: 30000,
	});

	const myTeamId = eventData?.data?.team_result?.team.id;

	if (isLoading) {
		return <Spinner size="large" />;
	}
	if (isError) {
		return <div>{error.response?.data.message ?? error.message}</div>;
	}

	return <ScoreBoard data={data?.data ?? []} myTeamId={myTeamId} className="mt-2" />;
}

function ScoreBoard({
	data,
	myTeamId,
	className,
}: {
	data: AwdScoreRow[];
	myTeamId?: string;
	className?: string;
}) {
	const columns = [
		{
			accessorKey: "rank",
			header: "Rank",
			field: "rank",
		},
		{
			accessorKey: "team_name",
			header: "Team",
			field: "team_name",
			rowHeader: true,
			renderCell: (row: AwdScoreRow) => {
				const isMyTeam = myTeamId && row.team_id === myTeamId;
				return (
					<div className="flex items-center gap-2">
						<div
							className={`flex items-center justify-center rounded-full font-medium shrink-0 ${
								isMyTeam
									? "bg-[var(--accent-emphasis)] text-[var(--fgColor-onEmphasis)]"
									: "bg-gray-200 text-gray-500"
							}`}
							style={{ width: 24, height: 24, fontSize: 10 }}
						>
							{row.team_name?.[0]?.toUpperCase() || "?"}
						</div>
						<span className={isMyTeam ? "font-semibold" : ""}>
							{row.team_name}
						</span>
					</div>
				);
			},
		},
		{
			accessorKey: "attack_score",
			header: "Attack",
			field: "attack_score",
			renderCell: (row: AwdScoreRow) => <span>{row.attack_score}</span>,
		},
		{
			accessorKey: "defense_score",
			header: "Defense",
			field: "defense_score",
			renderCell: (row: AwdScoreRow) => <span>{row.defense_score}</span>,
		},
		{
			accessorKey: "total_score",
			header: "Total",
			field: "total_score",
			renderCell: (row: AwdScoreRow) => <strong>{row.total_score}</strong>,
		},
	];

	const table = useReactTable({
		data,
		columns,
		getCoreRowModel: getCoreRowModel(),
	});

	return (
		<Table.Container className={`${className}`}>
			<Table.Subtitle id="awd-scoreboard-subtitle">
				<div className="flex gap-2">
					<span>Attack: flags submitted</span>
					<span>Defense: GameBox defense</span>
					<span>Total: combined score</span>
				</div>
			</Table.Subtitle>
			<DataTable
				aria-labelledby="awd-scoreboard"
				// @ts-ignore
				columns={columns}
				data={table.getRowModel().rows.map((row) => ({
					...row.original,
					id: row.original.team_id,
				}))}
			/>
			{data.length === 0 && (
				<p className="text-sm opacity-70 p-3">No scores yet.</p>
			)}
		</Table.Container>
	);
}