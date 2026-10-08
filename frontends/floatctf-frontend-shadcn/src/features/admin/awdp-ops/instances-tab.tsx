/**
 * 「实例」标签：AWDP 赛事实例列表（管理端 inspect 视图）。
 *
 * 已核实的后端语义：`GET /admin/events/{id}/awdp/instances` 经 run 聚合返回全部实例，
 * 含 `runtime_state`、`runtime_generation`、容器名与公开端点；DTO **不含 flag**。
 * 后端还返回 `reset_count`（玩家手动重置次数），但公共 SDK 类型未声明该字段，
 * 因此这里做一次带类型保护的可选读取（见交付报告的 PUBLIC SDK GAP）。
 */

import { RefreshCw } from "lucide-react";

import type { AwdpAdminInstanceDto } from "@floatctf/sdk";

import { TonePill } from "~/components/app/badges";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { MonoText, SectionCard } from "~/components/app/page";
import { EmptyBlock, QueryState, TableSkeleton } from "~/components/app/states";
import { Button } from "~/components/ui/button";

import { useAwdpInstances } from "./queries";

/** 后端返回但 SDK 未声明的字段：带类型保护地读取，拿不到就显示「—」。 */
function readResetCount(row: AwdpAdminInstanceDto): number | null {
	const value = (row as { reset_count?: unknown }).reset_count;
	return typeof value === "number" ? value : null;
}

function runtimeTone(state: string) {
	if (state === "running") return "success" as const;
	if (state === "resetting" || state === "pending") return "warning" as const;
	if (state === "stopped" || state === "exited") return "muted" as const;
	return "neutral" as const;
}

export function AwdpInstancesTab({ eventId }: { eventId: string }) {
	const instancesQuery = useAwdpInstances(eventId);

	const columns: DataTableColumn<AwdpAdminInstanceDto>[] = [
		{
			id: "gamebox_name",
			header: "GameBox",
			sortValue: (row) => row.gamebox_name,
			cell: (row) => (
				<div className="min-w-0">
					<p className="truncate font-medium">{row.gamebox_name}</p>
					<p className="text-xs text-muted-foreground">
						{row.owner_team_id ? "战队主体" : row.owner_user_id ? "个人主体" : "未绑定主体"}
					</p>
				</div>
			),
		},
		{
			id: "runtime_state",
			header: "运行时状态",
			cell: (row) => <TonePill tone={runtimeTone(row.runtime_state)}>{row.runtime_state}</TonePill>,
			sortValue: (row) => row.runtime_state,
		},
		{
			id: "runtime_generation",
			header: "代际",
			align: "right",
			cell: (row) => row.runtime_generation,
			sortValue: (row) => row.runtime_generation,
		},
		{
			id: "reset_count",
			header: "重置次数",
			align: "right",
			hideBelow: "md",
			cell: (row) => readResetCount(row) ?? "—",
			sortValue: (row) => readResetCount(row) ?? -1,
		},
		{
			id: "endpoints",
			header: "端点",
			hideBelow: "lg",
			cell: (row) =>
				row.endpoints.length === 0 ? (
					<span className="text-muted-foreground">—</span>
				) : (
					<div className="space-y-0.5">
						{row.endpoints.map((endpoint) => (
							<MonoText key={`${endpoint.protocol}-${endpoint.public_port}-${endpoint.container_port}`}>
								{endpoint.protocol}://{endpoint.public_host}:{endpoint.public_port} →{" "}
								{endpoint.container_port}
							</MonoText>
						))}
					</div>
				),
		},
		{
			id: "container_name",
			header: "容器名",
			hideBelow: "xl",
			cell: (row) => <MonoText>{row.container_name}</MonoText>,
		},
		{
			id: "instance_id",
			header: "实例 ID",
			hideBelow: "xl",
			cell: (row) => <MonoText>{row.instance_id}</MonoText>,
		},
	];

	return (
		<SectionCard
			title="赛事实例"
			description="全部参与者在本赛事的 AWDP 实例（含运行时状态与公开端点）。"
			actions={
				<Button
					variant="ghost"
					size="sm"
					disabled={instancesQuery.isFetching}
					onClick={() => void instancesQuery.refetch()}
				>
					<RefreshCw /> 刷新
				</Button>
			}
		>
			<QueryState
				query={instancesQuery}
				skeleton={<TableSkeleton rows={5} columns={6} />}
				isEmpty={(rows) => rows.length === 0}
				empty={
					<EmptyBlock
						title="还没有实例"
						description="比赛开始后平台会为参与者启动已挂载 GameBox 的实例。"
					/>
				}
			>
				{(rows) => (
					<DataTable
						data={rows}
						getRowId={(row) => row.instance_id}
						columns={columns}
						mobileCard={(row) => (
							<div className="space-y-1">
								<p className="text-sm font-medium">{row.gamebox_name}</p>
								<p className="text-xs text-muted-foreground">
									{row.runtime_state} · 代际 {row.runtime_generation}
								</p>
							</div>
						)}
					/>
				)}
			</QueryState>
		</SectionCard>
	);
}
