import { Avatar } from "@primer/react";
import { createFileRoute } from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { serviceApi } from "@/api";
import { GenericTable } from "@/components";
import type { SolveResult } from "@/api/service/solves";
import { AppLink } from "@/navigation";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/service/solves")({
	component: RouteComponent,
});

function RouteComponent() {
	useTitle("Solves | FloatCTF");
	const columns = [
		{ accessorKey: "id", header: "ID", field: "id", rowHeader: true },
		{
			accessorKey: "nickname",
			header: "User",
			field: "nickname",
			renderCell: (row: SolveResult) => (
				<div className="flex items-center gap-2">
					{row.avatar ? (
						<Avatar src={row.avatar} size={24} />
					) : (
						<div
							className="flex items-center justify-center rounded-full bg-gray-200 text-gray-500 font-medium flex-shrink-0"
							style={{ width: 24, height: 24, fontSize: 10 }}
						>
							{row.nickname?.[0]?.toUpperCase() || "?"}
						</div>
					)}
					<span>{row.nickname}</span>
				</div>
			),
		},
		{
			accessorKey: "challenge_id",
			header: "Challenge ID",
			field: "challenge_id",
			renderCell: (row: SolveResult) => (
				<AppLink
					to={"/service/challenges/$id"}
					params={{ id: row.challenge_id }}
				>
					{row.challenge_id}
				</AppLink>
			),
		},
		{
			accessorKey: "event_id",
			header: "Event ID",
			field: "event_id",
			renderCell: (row: SolveResult) => <span>{row.event_id ?? "—"}</span>,
		},
		{
			accessorKey: "created_at",
			header: "Created At",
			field: "created_at",
			renderCell: (row: SolveResult) => (
				<span>{DatetimeToShow(row.created_at)}</span>
			),
		},
	];
	const filterKeys = ["challenge_id", "event_id"];

	return (
		<GenericTable
			subject="Challenge Solves"
			columns={columns}
			filterKeys={filterKeys}
			queryFn={serviceApi.solves.fetch}
			enableInternalActions={false}
			disableAdd={true}
			disableSelect={true}
		/>
	);
}
