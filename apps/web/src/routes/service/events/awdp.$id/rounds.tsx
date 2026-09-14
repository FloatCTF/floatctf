import { Spinner } from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";
import dayjs from "dayjs";

import {
	awdpPlayerApi,
	type AwdpEvaluationDto,
	type AwdpRoundDto,
} from "@/api/awdp";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awdp/$id/rounds")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function fmt(iso: string | null | undefined) {
	return iso ? dayjs.utc(iso).local().format("MM-DD HH:mm:ss") : "-";
}

const EVAL_STATUS_LABEL: Record<string, string> = {
	pending: "pending",
	running: "running",
	no_patch: "no patch (+0)",
	service_down: "service down (+0)",
	functional_broken: "broken (+0)",
	vulnerable: "vulnerable (+0)",
	patched: "patched (+score)",
	platform_error: "platform error",
};

function RouteComponent() {
	const { id } = Route.useParams();
	const roundsQuery = useQuery({
		queryKey: ["awdp-rounds", id],
		queryFn: () => awdpPlayerApi.rounds(id),
	});
	const evalsQuery = useQuery({
		queryKey: ["awdp-evals", id],
		queryFn: () => awdpPlayerApi.evaluations(id),
	});

	const roundColumns = [
		{
			accessorKey: "sequence",
			header: "Turn",
			field: "sequence",
			rowHeader: true,
			renderCell: (row: AwdpRoundDto) => <span>#{row.sequence}</span>,
		},
		{
			accessorKey: "starts_at",
			header: "Starts",
			field: "starts_at",
			renderCell: (row: AwdpRoundDto) => <span>{fmt(row.starts_at)}</span>,
		},
		{
			accessorKey: "cutoff_at",
			header: "Cutoff",
			field: "cutoff_at",
			renderCell: (row: AwdpRoundDto) => <span>{fmt(row.cutoff_at)}</span>,
		},
		{
			accessorKey: "status",
			header: "Status",
			field: "status",
		},
	];
	const roundTable = useReactTable({
		data: roundsQuery.data?.data ?? [],
		columns: roundColumns,
		getCoreRowModel: getCoreRowModel(),
	});

	const evalColumns = [
		{
			accessorKey: "round_sequence",
			header: "Turn",
			field: "round_sequence",
			rowHeader: true,
			renderCell: (row: AwdpEvaluationDto) => (
				<span>{row.round_sequence ? `#${row.round_sequence}` : "-"}</span>
			),
		},
		{
			accessorKey: "kind",
			header: "Kind",
			field: "kind",
			renderCell: (row: AwdpEvaluationDto) => (
				<span className="text-xs opacity-70">{row.kind}</span>
			),
		},
		{
			accessorKey: "status",
			header: "Result",
			field: "status",
			renderCell: (row: AwdpEvaluationDto) => (
				<span>{EVAL_STATUS_LABEL[row.status] ?? row.status}</span>
			),
		},
		{
			accessorKey: "finished_at",
			header: "Finished",
			field: "finished_at",
			renderCell: (row: AwdpEvaluationDto) => (
				<span>{fmt(row.finished_at)}</span>
			),
		},
	];
	const evalTable = useReactTable({
		data: evalsQuery.data?.data ?? [],
		columns: evalColumns,
		getCoreRowModel: getCoreRowModel(),
	});

	if (roundsQuery.isLoading) {
		return <Spinner size="large" />;
	}

	return (
		<div className="m-2 flex flex-col gap-4">
			<section>
				<h4 className="font-bold mb-2">Fix Turns</h4>
				<Table.Container>
					<DataTable
						aria-labelledby="awdp-rounds"
						// @ts-ignore
						columns={roundColumns}
						data={roundTable.getRowModel().rows.map((row) => row.original)}
					/>
				</Table.Container>
			</section>

			<section>
				<h4 className="font-bold mb-2">My Evaluations</h4>
				<Table.Container>
					<DataTable
						aria-labelledby="awdp-evals"
						// @ts-ignore
						columns={evalColumns}
						data={evalTable.getRowModel().rows.map((row) => row.original)}
					/>
				</Table.Container>
				{(evalsQuery.data?.data?.length ?? 0) === 0 && (
					<p className="text-sm opacity-70 mt-2">暂无评估记录。</p>
				)}
			</section>
		</div>
	);
}
