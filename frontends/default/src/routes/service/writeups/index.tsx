import { Avatar, Label } from "@primer/react";
import { createFileRoute } from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { serviceApi } from "@/api";
import type { UnifiedWriteupResult } from "@floatctf/sdk";
import { GenericTable } from "@/components";
import { AppLink } from "@/navigation";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/service/writeups/")({
	component: RouteComponent,
});

function RouteComponent() {
	useTitle("Writeups | FloatCTF");
	const subject = "Writeups";
	const filterKeys = ["id", "challenge_id"];

	const columns = [
		{
			accessorKey: "content_name",
			header: "内容",
			field: "content_name",
			rowHeader: true,
			sortBy: true,
			renderCell: (row: UnifiedWriteupResult) => (
				<AppLink to="/service/writeups/$id" params={{ id: row.id }}>
					{row.content_name}
				</AppLink>
			),
		},
		{
			accessorKey: "nickname",
			header: "作者",
			field: "nickname",
			sortBy: true,
			renderCell: (row: UnifiedWriteupResult) => (
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
			accessorKey: "writeup_type",
			header: "类型",
			field: "writeup_type",
			renderCell: (row: UnifiedWriteupResult) => (
				<Label variant={row.writeup_type === "gamebox" ? "success" : "accent"}>
					{row.writeup_type === "gamebox" ? "Gamebox" : "Challenge"}
				</Label>
			),
		},
		{
			accessorKey: "email",
			header: "邮箱",
			field: "email",
			renderCell: (row: UnifiedWriteupResult) => (
				<a href={`mailto:${row.email}`}>{row.email}</a>
			),
		},
		{
			accessorKey: "updated_at",
			header: "更新时间",
			field: "updated_at",
			sortBy: true,
			renderCell: (row: UnifiedWriteupResult) => (
				<span>{DatetimeToShow(row.updated_at)}</span>
			),
		},
	];

	return (
		<GenericTable
			subject={subject}
			columns={columns}
			filterKeys={filterKeys}
			queryFn={serviceApi.challenges.getAllWriteups}
			enableInternalActions={false}
			disableAdd={true}
			disableSelect={true}
			getRowId={(row: UnifiedWriteupResult) => row.id}
		/>
	);
}
