import { Label } from "@primer/react";

import { instanceAdminApi, type AdminInstanceRow } from "@/api/admin/instances";
import { GenericTable } from "@/components";
import { DatetimeToShow } from "@/util";

/**
 * Admin 赛事 Instance Tab 共享组件（归一化 event_instances 视图）。
 * 供 jeopardy / awd / awdp 三个赛制的 admin 赛事详情页复用：
 * 展示该赛事的全部实例（challenge / AWD / AWDP 关联），Content 列显示对应 title。
 */
export function EventInstancesTable({ eventId }: { eventId: string }) {
	const columns = [
		{
			accessorKey: "instance_type",
			header: "类型",
			field: "instance_type",
			rowHeader: true,
			renderCell: (row: AdminInstanceRow) => (
				<Label variant={row.instance_type === "challenge" ? "accent" : "success"}>
					{row.instance_type === "challenge" ? "Challenge" : "GameBox"}
				</Label>
			),
		},
		{
			accessorKey: "status",
			header: "状态",
			field: "status",
		},
		{
			accessorKey: "identifier",
			header: "标识",
			field: "identifier",
			renderCell: (row: AdminInstanceRow) => (
				<span className="font-mono text-xs">{row.identifier}</span>
			),
		},
		{
			accessorKey: "content_title",
			header: "内容",
			field: "content_title",
			renderCell: (row: AdminInstanceRow) => (
				<span>{row.content_title ?? row.challenge_id ?? row.gamebox_id ?? "-"}</span>
			),
		},
		{
			accessorKey: "user_name",
			header: "用户",
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
			accessorKey: "runtime_generation",
			header: "代数",
			field: "runtime_generation",
			renderCell: (row: AdminInstanceRow) => (
				<span>{row.runtime_generation ?? "-"}</span>
			),
		},
		{
			accessorKey: "destroy_at",
			header: "销毁时间",
			field: "destroy_at",
			renderCell: (row: AdminInstanceRow) => (
				<span>{DatetimeToShow(row.destroy_at)}</span>
			),
		},
	];

	return (
		<div className="m-2">
			<p className="text-sm opacity-80 mb-2">
				归一化实例（event_instances 根表）：本赛事的全部实例，Content 为对应题目 / GameBox 名称。
			</p>
			<GenericTable
				subject={`event-instances:${eventId}`}
				columns={columns}
				filterKeys={["status", "identifier", "user_id"]}
				queryFn={(params) => instanceAdminApi.listForEvent(eventId, params)}
				enableInternalActions={false}
				disableAdd={true}
				disableSelect={true}
			/>
		</div>
	);
}
