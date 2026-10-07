import { createFileRoute } from "@tanstack/react-router";

import { serviceApi } from "@/api";
import { GenericTable } from "@/components";
import type { ChallengeSets } from "@floatctf/sdk/entity";
import { AppLink } from "@/navigation";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/service/challenge_sets/")({
	component: RouteComponent,
});

function RouteComponent() {
	const subject = "Challenge Sets";
	const columns = [
		{
			accessorKey: "name",
			header: "名称",
			field: "name",
			rowHeader: true,
			renderCell: (row: ChallengeSets) => {
				return (
					<AppLink to="/service/challenge_sets/$id" params={{ id: row.id }}>
						{row.name}
					</AppLink>
				);
			},
			sortBy: true,
		},
		{
			accessorKey: "description",
			header: "描述",
			field: "description",
			sortBy: true,
		},
		{
			accessorKey: "updated_at",
			header: "更新时间",
			field: "updated_at",
			sortBy: true,
			renderCell: (row: ChallengeSets) => {
				return <span>{DatetimeToShow(row.updated_at)}</span>;
			},
		},
	];
	const filterKeys = ["name", "description"];

	return (
		<GenericTable
			subject={subject}
			columns={columns}
			filterKeys={filterKeys}
			queryFn={serviceApi.challenges.getChallengeSets}
			disableAdd={true}
			disablePagination={true}
			disableSelect={true}
			enableInternalActions={false}
		/>
	);
}
