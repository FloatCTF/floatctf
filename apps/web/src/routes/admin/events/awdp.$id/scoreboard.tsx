import { Spinner } from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";

import { type AwdpScoreRow, awdpAdminApi } from "@/api/awdp";
import { AdminRouteGuard } from "../../route";

/**
 * AWDP 赛事管理端 Scoreboard（独立 tab）。
 *
 * 完整榜单（Rank / 参与者 / Break / Fix / Total），数据来自
 * `GET /api/admin/events/{id}/awdp/scores`，30s 轮询。
 * 原先是 Ops 面板的内嵌区块，按用户要求独立成页（对齐选手端 Scoreboard tab）。
 */
export const Route = createFileRoute("/admin/events/awdp/$id/scoreboard")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const scoresQuery = useQuery({
		queryKey: ["admin-awdp-scores", id],
		queryFn: () => awdpAdminApi.scores(id),
		refetchInterval: 30000,
	});
	const scoreRows = scoresQuery.data?.data ?? [];

	const columns = [
		{ accessorKey: "rank", header: "Rank", field: "rank" },
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
			renderCell: (row: AwdpScoreRow) => (
				<span className="tabular-nums">{row.break_score}</span>
			),
		},
		{
			accessorKey: "fix_score",
			header: "Fix",
			field: "fix_score",
			renderCell: (row: AwdpScoreRow) => (
				<span className="tabular-nums">{row.fix_score}</span>
			),
		},
		{
			accessorKey: "total_score",
			header: "Total",
			field: "total_score",
			renderCell: (row: AwdpScoreRow) => (
				<strong className="tabular-nums">{row.total_score}</strong>
			),
		},
	];

	const table = useReactTable({
		data: scoreRows,
		columns,
		getCoreRowModel: getCoreRowModel(),
	});

	if (scoresQuery.isLoading) {
		return <Spinner size="large" />;
	}
	if (scoresQuery.isError) {
		return <div>Failed to load scoreboard.</div>;
	}

	return (
		<div className="m-2">
			<Table.Container>
				<Table.Subtitle id="admin-awdp-scoreboard-subtitle">
					<div className="flex gap-2">
						<span>Break: 攻破他人 GameBox 获得</span>
						<span>Fix: 修复成功（官方 check PATCHED）获得</span>
						<span>Total: 总分</span>
					</div>
				</Table.Subtitle>
				<DataTable
					aria-labelledby="admin-awdp-scoreboard"
					// @ts-ignore
					columns={columns}
					data={table.getRowModel().rows.map((row) => ({
						...row.original,
						id: row.original.subject_id,
					}))}
				/>
				{scoreRows.length === 0 && (
					<p className="text-sm opacity-70 mt-2">暂无成绩。</p>
				)}
			</Table.Container>
		</div>
	);
}
