import { createFileRoute } from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { serviceApi } from "@/api";
import { GenericTable } from "@/components";
import { AppLink } from "@/navigation";
import { ServiceRouteGuard } from "@/routes/service/route";
import type { ChallengesListItem } from "@floatctf/sdk";
import { DatetimeToShow } from "@/util";
import { CheckIcon } from "@primer/octicons-react";

export const Route = createFileRoute("/service/challenges/")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function RouteComponent() {
	useTitle("Challenges | FloatCTF");
	const columns = [
		{
			accessorKey: "name",
			header: "名称",
			field: "name",
			rowHeader: true,
			renderCell: (row: ChallengesListItem) => {
				return (
					<AppLink to={"/service/challenges/$id"} params={{ id: row.id }}>
						{row.name}
					</AppLink>
				);
			},
		},

		{
			accessorKey: "category",
			header: "分类",
			field: "category",
		},
		{
			accessorKey: "solved",
			header: "已解出",
			field: "solved",
			renderCell: (row: ChallengesListItem) => {
				return row.solved ? (
					<CheckIcon size={16} fill="var(--fgColor-success)" />
				) : null;
			},
		},
		{
			accessorKey: "author",
			header: "作者",
			field: "author",
			renderCell: (row: ChallengesListItem) => {
				return <span>{row.author || "—"}</span>;
			},
		},
		{
			accessorKey: "version",
			header: "版本",
			field: "version",
			renderCell: (row: ChallengesListItem) => {
				return <span>{row.version ?? "—"}</span>;
			},
		},
		{
			accessorKey: "updated_at",
			header: "更新时间",
			field: "updated_at",
			renderCell: (row: ChallengesListItem) => {
				return <span>{DatetimeToShow(row.updated_at)}</span>;
			},
		},
	];
	const filterKeys = ["name", "category", "description", "solved"];

	return (
		<GenericTable
			subject="ChallengesListItem"
			subtitle="If you want submit yours, pls visit https://github.com/FloatCTF/challenge-template"
			columns={columns}
			filterKeys={filterKeys}
			queryFn={serviceApi.challenges.fetch}
			enableInternalActions={false}
			disableAdd={true}
			disableSelect={true}
		/>
	);
}
