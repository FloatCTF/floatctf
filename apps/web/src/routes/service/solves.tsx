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
		{
			accessorKey: "challenge_name",
			header: "Challenge",
			field: "challenge_name",
			rowHeader: true,
			renderCell: (row: SolveResult) => (
				<AppLink
					to={"/service/challenges/$id"}
					params={{ id: row.challenge_id }}
				>
					{row.challenge_name ?? row.challenge_id}
				</AppLink>
			),
		},
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
			accessorKey: "updated_at",
			header: "Updated At",
			field: "updated_at",
			renderCell: (row: SolveResult) => (
				<span>{DatetimeToShow(row.updated_at)}</span>
			),
		},
	];
	const filterKeys = ["challenge_id"];

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
