import { Spinner } from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";

import { awdpAdminApi, type AwdpAdminInstanceDto } from "@/api/awdp";
import { useAwdpEventStream } from "@/hooks/useAwdpEventStream";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/awdp/$id/instances")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	// 实时事件刷新 + 兜底轮询。
	useAwdpEventStream({ eventId: id });
	const { data, isLoading } = useQuery({
		queryKey: ["awdp-admin-instances", id],
		queryFn: () => awdpAdminApi.listInstances(id),
		refetchInterval: 5000,
	});

	const columns = [
		{
			accessorKey: "container_name",
			header: "Container",
			field: "container_name",
			rowHeader: true,
			renderCell: (row: AwdpAdminInstanceDto) => (
				<span className="font-mono text-xs">{row.container_name}</span>
			),
		},
		{
			accessorKey: "gamebox_name",
			header: "GameBox",
			field: "gamebox_name",
		},
		{
			accessorKey: "owner",
			header: "Owner",
			field: "owner",
			renderCell: (row: AwdpAdminInstanceDto) => (
				<span className="font-mono text-xs">
					{row.owner_user_id
						? `user:${row.owner_user_id.slice(0, 8)}…`
						: row.owner_team_id
							? `team:${row.owner_team_id.slice(0, 8)}…`
							: "-"}
				</span>
			),
		},
		{
			accessorKey: "runtime_state",
			header: "State",
			field: "runtime_state",
		},
		{
			accessorKey: "runtime_generation",
			header: "Gen",
			field: "runtime_generation",
		},
		{
			accessorKey: "endpoints",
			header: "Endpoints",
			field: "endpoints",
			renderCell: (row: AwdpAdminInstanceDto) => (
				<span className="font-mono text-xs">
					{row.endpoints
						.map((e) => `${e.protocol}://${e.public_host}:${e.public_port}`)
						.join(" ") || "-"}
				</span>
			),
		},
	];

	const table = useReactTable({
		data: data?.data ?? [],
		columns,
		getCoreRowModel: getCoreRowModel(),
	});

	if (isLoading) {
		return <Spinner size="large" />;
	}

	return (
		<div className="m-2">
			<p className="text-sm opacity-80 mb-2">
				全部选手实例；公开端口跨 reset 保持稳定，runtime_generation 每次重建 +1。
			</p>
			<Table.Container>
				<DataTable
					aria-labelledby="awdp-instances"
					// @ts-ignore
					columns={columns}
					data={table
						.getRowModel()
						.rows.map((row) => ({
							...row.original,
							id: row.original.instance_id,
						}))}
				/>
			</Table.Container>
			{(data?.data?.length ?? 0) === 0 && (
				<p className="text-sm opacity-70 mt-2">暂无实例。</p>
			)}
		</div>
	);
}
