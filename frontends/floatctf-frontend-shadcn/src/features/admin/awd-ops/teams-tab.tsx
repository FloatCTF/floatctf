/**
 * 「战队」标签：AWD 战队封禁 / 解封。
 *
 * 已核实的后端语义（`ban_service`）：
 * - 封禁：写入 `awd_team_bans`（含原因与操作管理员）→ 同步 `event_teams.banned` →
 *   挂起该队在宿主上的 WireGuard 对端（DB 记录保留）→ 重建防火墙 banned 集合 → 清理该队连接；
 * - 解封：反向闭环（DB 解除 → 恢复 WG 对端 → 重建 banned 集合）；
 * - 封禁是手动的，**不会自动到期**；解封前该队一直失去比赛访问权限。
 */

import { useState, type FormEvent } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Ban, UserMinus, Users } from "lucide-react";

import type { TeamResult } from "@floatctf/sdk";

import { call, callVoid } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { Field, FormFooter, FormSheet } from "~/components/app/form";
import { MonoText, SectionCard } from "~/components/app/page";
import { EmptyBlock, QueryState, TableSkeleton } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Textarea } from "~/components/ui/textarea";
import { formatDateTime } from "~/lib/format";

import { useAwdTeams } from "./queries";

/** 封禁表单：收集原因（后端会连同操作管理员一起写入封禁记录）。 */
function BanTeamSheet({ eventId, team, onClose }: { eventId: string; team: TeamResult; onClose: () => void }) {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const [reason, setReason] = useState("");
	const [error, setError] = useState<string | null>(null);

	const mutation = useMutation({
		mutationFn: (input: { reason: string }) =>
			call<string>(
				client.awd.admin.banTeam(eventId, team.team.id, { reason: input.reason }),
				"封禁战队",
			),
		onSuccess: () => {
			toast.success(`已封禁「${team.team.name}」`, "该队已失去比赛网络访问权限，需手动解封。");
			void queryClient.invalidateQueries({ queryKey: qk.admin.eventTeams(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.awd.adminScores(eventId) });
			onClose();
		},
		onError: (error) => toast.apiError("封禁战队失败", error),
	});

	const submit = async (event: FormEvent) => {
		event.preventDefault();
		const trimmed = reason.trim();
		if (trimmed === "") {
			setError("请填写封禁原因（会写入封禁记录，便于赛后复盘）");
			return;
		}
		setError(null);
		const ok = await confirm({
			title: `封禁「${team.team.name}」？`,
			description: "封禁会立即切断该队的比赛网络访问，且不会自动解除。",
			consequences: [
				"写入封禁记录（原因 + 操作管理员），并同步战队的 banned 标志",
				"挂起该队在宿主上的 WireGuard 对端，重建防火墙 banned 集合并清理该队网段连接",
				"该队无法提交 flag、访问靶机（SSH）或重置实例，直到管理员手动解封",
			],
			tone: "danger",
			confirmText: "封禁",
		});
		if (ok) mutation.mutate({ reason: trimmed });
	};

	return (
		<FormSheet
			open
			onOpenChange={(open) => {
				if (!open) onClose();
			}}
			title={`封禁战队：${team.team.name}`}
			description="封禁是手动解除的，不会自动到期。"
			footer={<FormFooter onCancel={onClose} formId="awd-ban-form" submitLabel="封禁" isPending={mutation.isPending} />}
		>
			<form id="awd-ban-form" onSubmit={(event) => void submit(event)} className="space-y-4">
				<Field label="封禁原因" htmlFor="awd-ban-reason" required error={error} hint="写入封禁记录，供赛后审计">
					<Textarea
						id="awd-ban-reason"
						rows={3}
						value={reason}
						disabled={mutation.isPending}
						onChange={(event) => setReason(event.target.value)}
						placeholder="例如：越权攻击平台基础设施"
					/>
				</Field>
			</form>
		</FormSheet>
	);
}

export function AwdTeamsTab({ eventId }: { eventId: string }) {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const teamsQuery = useAwdTeams(eventId);
	const [banTarget, setBanTarget] = useState<TeamResult | null>(null);

	const unban = useMutation({
		mutationFn: (team: TeamResult) =>
			callVoid(client.awd.admin.unbanTeam(eventId, team.team.id), "解封战队"),
		onSuccess: (_result, team) => {
			toast.success(`已解封「${team.team.name}」`);
			void queryClient.invalidateQueries({ queryKey: qk.admin.eventTeams(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.awd.adminScores(eventId) });
		},
		onError: (error) => toast.apiError("解封战队失败", error),
	});

	const columns: DataTableColumn<TeamResult>[] = [
		{
			id: "name",
			header: "战队",
			sortValue: (row) => row.team.name,
			cell: (row) => <span className="font-medium">{row.team.name}</span>,
		},
		{
			id: "banned",
			header: "状态",
			cell: (row) => (
				<TonePill tone={row.team.banned ? "danger" : "success"}>
					{row.team.banned ? "已封禁" : "正常"}
				</TonePill>
			),
			sortValue: (row) => (row.team.banned ? 1 : 0),
		},
		{
			id: "captain",
			header: "队长",
			hideBelow: "md",
			cell: (row) => row.captain || "—",
		},
		{
			id: "members",
			header: "成员",
			align: "right",
			cell: (row) => row.members.length,
			sortValue: (row) => row.members.length,
		},
		{
			id: "created_at",
			header: "创建时间",
			hideBelow: "lg",
			cell: (row) => formatDateTime(row.team.created_at, { seconds: true }),
			sortValue: (row) => row.team.created_at,
		},
		{
			id: "team_id",
			header: "战队 ID",
			hideBelow: "lg",
			cell: (row) => <MonoText>{row.team.id}</MonoText>,
		},
	];

	return (
		<SectionCard
			title="参赛战队"
			description="封禁会切断该队的比赛网络访问（WireGuard / 防火墙 / 连接清理），需手动解封。"
		>
			<QueryState
				query={teamsQuery}
				skeleton={<TableSkeleton rows={6} columns={5} />}
				isEmpty={(teams) => teams.length === 0}
				empty={
					<EmptyBlock
						title="还没有战队"
						description="AWD 是战队赛：先在赛事控制台创建战队并加入成员，这里才会出现。"
						icon={<Users className="size-5" />}
					/>
				}
			>
				{(teams) => (
					<>
						<DataTable
							data={teams}
							getRowId={(row) => row.team.id}
							columns={columns}
							mobileCard={(row) => (
								<div className="flex items-center justify-between gap-2">
									<div className="min-w-0">
										<p className="truncate text-sm font-medium">{row.team.name}</p>
										<p className="text-xs text-muted-foreground">{row.members.length} 名成员</p>
									</div>
									<TonePill tone={row.team.banned ? "danger" : "success"}>
										{row.team.banned ? "已封禁" : "正常"}
									</TonePill>
								</div>
							)}
							rowActions={(row) =>
								row.team.banned ? (
									<Button
										variant="outline"
										size="xs"
										disabled={unban.isPending}
										onClick={async () => {
											const ok = await confirm({
												title: `解封「${row.team.name}」？`,
												description: "恢复该队的比赛网络访问。",
												consequences: [
													"恢复该队在宿主上的 WireGuard 对端与防火墙放行",
													"解封后该队可继续提交 flag、访问靶机与重置实例",
													"写入 TeamUnbanned 审计记录",
												],
												confirmText: "解封",
											});
											if (ok) unban.mutate(row);
										}}
									>
										<UserMinus /> 解封
									</Button>
								) : (
									<Button
										variant="destructive"
										size="xs"
										onClick={() => setBanTarget(row)}
										aria-label={`封禁 ${row.team.name}`}
									>
										<Ban /> 封禁
									</Button>
								)
							}
						/>
						{banTarget ? (
							<BanTeamSheet eventId={eventId} team={banTarget} onClose={() => setBanTarget(null)} />
						) : null}
					</>
				)}
			</QueryState>
		</SectionCard>
	);
}
