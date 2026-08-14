import type { UniResponse } from "@/api/axios";
import { Spinner } from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";
import type { AxiosError } from "axios";

import { type AwdpScoreRow, awdpPlayerApi } from "@/api/awdp";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awdp/$id/scoreboard")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const { data, isLoading, isError, error } = useQuery<
		UniResponse<AwdpScoreRow[]>,
		AxiosError<{ message: string }>
	>({
		queryKey: ["awdp-scores", id],
		queryFn: () => awdpPlayerApi.scores(id),
		refetchInterval: 30000, // 30 秒自动刷新（回合推进分数变化）
	});

	if (isLoading) {
		return <Spinner size="large" />;
	}
	if (isError) {
		return <div>{error.response?.data.message ?? error.message}</div>;
	}

	return <ScoreBoard data={data?.data ?? []} className="mt-2" />;
}

export function ScoreBoard({
	data,
	className,
}: {
	data: AwdpScoreRow[];
	className?: string;
}) {
	const columns = [
		{
			accessorKey: "rank",
			header: "Rank",
			field: "rank",
		},
		{
			accessorKey: "subject_name",
			header: "Participant",
			field: "subject_name",
			rowHeader: true,
			renderCell: (row: AwdpScoreRow) => (
				<div className="flex items-center gap-2">
					<div
						className="flex items-center justify-center rounded-full bg-gray-200 text-gray-500 font-medium shrink-0"
						style={{ width: 24, height: 24, fontSize: 10 }}
					>
						{row.subject_name?.[0]?.toUpperCase() || "?"}
					</div>
					<span>{row.subject_name}</span>
				</div>
			),
		},
		{
			accessorKey: "break_score",
			header: "Break",
			field: "break_score",
			renderCell: (row: AwdpScoreRow) => <span>{row.break_score}</span>,
		},
		{
			accessorKey: "fix_score",
			header: "Fix",
			field: "fix_score",
			renderCell: (row: AwdpScoreRow) => <span>{row.fix_score}</span>,
		},
		{
			accessorKey: "total_score",
			header: "Total",
			field: "total_score",
			renderCell: (row: AwdpScoreRow) => <strong>{row.total_score}</strong>,
		},
	];

	const table = useReactTable({
		data,
		columns,
		getCoreRowModel: getCoreRowModel(),
	});

	return (
		<Table.Container className={`${className}`}>
			<Table.Subtitle id="awdp-scoreboard-subtitle">
				<div className="flex gap-2">
					<span>Break: 攻破他人 GameBox 获得</span>
					<span>Fix: 修复成功（官方 check PATCHED）获得</span>
					<span>Total: 总分</span>
				</div>
			</Table.Subtitle>
			<DataTable
				aria-labelledby="awdp-scoreboard"
				// @ts-ignore
				columns={columns}
				data={table.getRowModel().rows.map((row) => ({
					...row.original,
					id: row.original.subject_id,
				}))}
			/>
			{data.length === 0 && (
				<p className="text-sm opacity-70 p-3">暂无成绩。</p>
			)}
		</Table.Container>
	);
}
