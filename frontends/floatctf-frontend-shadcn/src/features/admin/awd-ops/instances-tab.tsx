/**
 * 「实例」标签：赛事 GameBox 实例列表与管理员重置（specialized）。
 *
 * 已核实的后端语义：
 * - 实例列表来自管理端归一化视图 `GET /admin/events/{id}/instances`（`AdminInstanceRow`，
 *   不返回 flag，`instance_type == "gamebox"` 才是 AWD 靶机）；
 * - `POST .../awd/gameboxes/{instance_id}/reset` 的 `instance_id` 就是该视图的 `id`；
 * - 重置资格由后端 `check_reset_eligibility` 决定：必须 status == running 且
 *   phase ∈ {hardening, attack}（暂停 / 终局结算均不允许）；
 * - 管理员重置不扣战队免费重置次数（charge_team=false），但会写入重置记录与审计。
 */

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { RefreshCw, RotateCcw } from "lucide-react";

import type { AdminInstanceRow } from "@floatctf/sdk";

import { callList, callVoid } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { MonoText, SectionCard } from "~/components/app/page";
import { EmptyBlock, QueryState, TableSkeleton } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { formatDateTime } from "~/lib/format";

import { useAwdStatus } from "./queries";
import { awdStatusLabel } from "./shared";

const RESETTABLE_PHASES = ["hardening", "attack"];

export function AwdInstancesTab({ eventId }: { eventId: string }) {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const statusQuery = useAwdStatus(eventId);

	const instancesQuery = useQuery({
		queryKey: qk.admin.eventInstances(eventId),
		queryFn: () =>
			callList<AdminInstanceRow>(
				client.admin.instances.listForEvent(eventId, { limit: 200 }),
			).then((result) => result.items),
		enabled: eventId !== "",
	});

	const status = statusQuery.data ?? null;
	const resetAllowed =
		status !== null &&
		status.status === "running" &&
		RESETTABLE_PHASES.includes(status.phase) &&
		!status.final_settlement;

	const blockedReason =
		status === null
			? "尚未开通 AWD，没有可重置的实例"
			: status.status !== "running"
				? `赛事当前状态为「${awdStatusLabel(status.status)}」：只有进行中的赛事允许重置实例`
				: status.phase === "pause"
					? "赛事已暂停：暂停期间不允许重置实例"
					: status.final_settlement
						? "终局结算中：最后一轮已完成，不再允许重置实例"
						: "重置该实例（销毁并重建容器，回到初始状态）";

	const reset = useMutation({
		mutationFn: (row: AdminInstanceRow) =>
			callVoid(client.awd.admin.resetGamebox(eventId, row.id), "重置 GameBox 实例"),
		onSuccess: (_result, row) => {
			toast.success(`已重置实例`, row.content_title ?? row.identifier);
			void queryClient.invalidateQueries({ queryKey: qk.admin.eventInstances(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.awd.adminStatus(eventId) });
		},
		onError: (error) => toast.apiError("重置实例失败", error),
	});

	const rows = (instancesQuery.data ?? []).filter((row) => row.instance_type === "gamebox");

	const columns: DataTableColumn<AdminInstanceRow>[] = [
		{
			id: "content_title",
			header: "GameBox",
			sortValue: (row) => row.content_title ?? "",
			cell: (row) => (
				<div className="min-w-0">
					<p className="truncate font-medium">{row.content_title ?? "—"}</p>
					<p className="text-xs text-muted-foreground">
						{row.team_name ?? row.user_name ?? "未绑定主体"}
					</p>
				</div>
			),
		},
		{
			id: "status",
			header: "运行时状态",
			cell: (row) => (
				<TonePill
					tone={
						row.status === "running"
							? "success"
							: row.status === "resetting"
								? "warning"
								: "neutral"
					}
				>
					{row.status}
				</TonePill>
			),
			sortValue: (row) => row.status,
		},
		{
			id: "runtime_generation",
			header: "代际",
			align: "right",
			hideBelow: "md",
			cell: (row) => row.runtime_generation ?? "—",
			sortValue: (row) => row.runtime_generation ?? 0,
		},
		{
			id: "identifier",
			header: "容器名",
			hideBelow: "lg",
			cell: (row) => <MonoText>{row.identifier}</MonoText>,
		},
		{
			id: "updated_at",
			header: "更新时间",
			hideBelow: "xl",
			cell: (row) => formatDateTime(row.updated_at, { seconds: true }),
			sortValue: (row) => row.updated_at,
		},
		{
			id: "instance_id",
			header: "实例 ID",
			hideBelow: "xl",
			cell: (row) => <MonoText>{row.id}</MonoText>,
		},
	];

	return (
		<SectionCard
			title="GameBox 实例"
			description="赛事全部靶机实例（归一化视图）；管理员重置会重建容器并回到初始状态。"
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
				isEmpty={() => rows.length === 0}
				empty={
					<EmptyBlock
						title="还没有 GameBox 实例"
						description="部署赛事并开赛后，平台会为各战队创建 GameBox 实例。"
					/>
				}
			>
				{() => (
					<div className="space-y-3">
						{!resetAllowed ? (
							<p className="text-xs text-muted-foreground">重置当前不可用：{blockedReason}</p>
						) : null}
						<DataTable
							data={rows}
							getRowId={(row) => row.id}
							columns={columns}
							mobileCard={(row) => (
								<div className="space-y-1">
									<p className="text-sm font-medium">{row.content_title ?? "—"}</p>
									<p className="text-xs text-muted-foreground">
										{row.team_name ?? row.user_name ?? "未绑定主体"} · {row.status}
									</p>
								</div>
							)}
							rowActions={(row) => (
								<Button
									variant="outline"
									size="xs"
									disabled={!resetAllowed || reset.isPending}
									title={blockedReason}
									onClick={async () => {
										const ok = await confirm({
											title: `重置实例「${row.content_title ?? row.identifier}」？`,
											description: "重置会销毁并重建该实例的容器。",
											consequences: [
												"容器被停止、删除并按当前配置重建，实例回到初始状态（flag 重新生成）",
												"实例代际（runtime_generation）+1",
												"管理员重置不扣战队的免费重置次数，但会写入重置记录与审计",
												"重置期间该实例短暂不可用",
											],
											tone: "danger",
											confirmText: "重置",
										});
										if (ok) reset.mutate(row);
									}}
								>
									<RotateCcw /> 重置
								</Button>
							)}
						/>
					</div>
				)}
			</QueryState>
		</SectionCard>
	);
}
