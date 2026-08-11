import { Label } from "@primer/react";
import { createFileRoute } from "@tanstack/react-router";

import { adminApi } from "@/api";
import type { AdminInstanceRow } from "@/api/admin/instances";
import { GenericTable } from "@/components";
import { AdminRouteGuard } from "@/routes/admin/route";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/admin/instances")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

// 归一化实例列表：challenge 实例 + AWD/AWDP gamebox 实例。
// Content 列显示对应 title（challenge 名 / GameBox 名）；不再展示 flag。
function RouteComponent() {
	const columns = [
		{ accessorKey: "id", header: "ID", field: "id", rowHeader: true },
		{
			accessorKey: "status",
			header: "Status",
			field: "status",
			sortBy: true,
		},
		{
			accessorKey: "identifier",
			header: "Identifier",
			field: "identifier",
			sortBy: true,
		},
		{
			accessorKey: "event_title",
			header: "Event",
			field: "event_id",
			sortBy: true,
			renderCell: (row: AdminInstanceRow) => {
				return <span>{row.event_title ?? row.event_id ?? "-"}</span>;
			},
		},
		{
			accessorKey: "user_name",
			header: "User",
			field: "user_id",
			renderCell: (row: AdminInstanceRow) => {
				const name = row.user_name ?? row.team_name ?? "";
				const fallback = row.user_id ?? row.team_id ?? "-";
				return (
					<span>
						{name || fallback}
						{row.team_name && !row.user_name && (
							<Label variant="attention" className="ml-2">
								team
							</Label>
						)}
					</span>
				);
			},
		},
		{
			accessorKey: "content_title",
			header: "Content",
			field: "content_title",
			renderCell: (row: AdminInstanceRow) => {
				return (
					<span>
						{row.content_title ?? row.challenge_id ?? row.gamebox_id ?? "-"}
					</span>
				);
			},
		},
		{
			accessorKey: "destroy_at",
			header: "Destroy At",
			field: "destroy_at",
			renderCell: (row: AdminInstanceRow) => {
				return (
					<span>{row.destroy_at ? DatetimeToShow(row.destroy_at) : "-"}</span>
				);
			},
		},
	];

	const filterKeys = [
		"id",
		"status",
		"identifier",
		"event_id",
		"challenge_id",
		"gamebox_id",
		"user_id",
	];
	return (
		<GenericTable
			subject="Instances"
			columns={columns}
			filterKeys={filterKeys}
			queryFn={adminApi.instances.fetch}
			disableAdd={true}
			enableInternalActions={false}
		/>
	);
}
