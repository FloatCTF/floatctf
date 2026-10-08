/**
 * 赛事控制台 · 战队标签（event_teams）。
 *
 * `getTeams(eventId)` 是**柯里化且第二层无参数**：`client.admin.event_teams.getTeams(eventId)()`。
 * 该接口不接受 `page`/`limit`（一次返回全量 + `meta.total`），因此这里做客户端分页。
 */

import { useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Ban, MoreHorizontal, ShieldCheck, Trash2, Users } from "lucide-react";

import { call, callList } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { BooleanPill, TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import type { DataTableColumn } from "~/components/app/data-table";
import { MonoText, SectionCard, Toolbar } from "~/components/app/page";
import { EmptyBlock } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "~/components/ui/dropdown-menu";
import { EventTeamMemberRole } from "@floatctf/sdk/entity";
import type { TeamMemberResult, TeamResult } from "@floatctf/sdk";
import { formatDateTime } from "~/lib/format";

import { clientPageCount, clientPaged, MobileFacts, PagedTable } from "./shared";

function memberLabel(member: TeamMemberResult): string {
	return member.nickname && member.nickname.length > 0 ? member.nickname : member.username;
}

function MemberList({ members }: { members: TeamMemberResult[] }): ReactNode {
	if (members.length === 0) {
		return <span className="text-xs text-muted-foreground">无成员</span>;
	}
	return (
		<ul className="space-y-1">
			{members.map((member) => (
				<li key={member.username} className="flex flex-wrap items-center gap-2 text-xs">
					<span className="font-medium">{memberLabel(member)}</span>
					<TonePill tone={member.role === EventTeamMemberRole.Captain ? "info" : "muted"}>
						{member.role === EventTeamMemberRole.Captain ? "队长" : "成员"}
					</TonePill>
					<MonoText>{member.points.toFixed(2)}</MonoText>
				</li>
			))}
		</ul>
	);
}

export function EventTeamsTab({
	eventId,
	participantMode,
}: {
	eventId: string;
	participantMode: string;
}): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(20);
	const [selectedIds, setSelectedIds] = useState<string[]>([]);

	const query = useQuery({
		queryKey: qk.admin.eventTeams(eventId),
		queryFn: () => callList<TeamResult>(client.admin.event_teams.getTeams(eventId)()),
	});

	const total = query.data?.meta?.total ?? query.data?.items.length ?? 0;
	const safePage = Math.min(page, clientPageCount(total, pageSize));
	const pagedQuery = clientPaged(query, safePage, pageSize);

	function invalidate() {
		void queryClient.invalidateQueries({ queryKey: qk.admin.eventTeams(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.admin.eventUsers(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.admin.event(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.admin.eventLogs(eventId) });
	}

	// 后端没有批量封禁接口：逐条串行调用，成功后统一提示与失效。
	const ban = useMutation({
		mutationFn: async (teamIds: string[]) => {
			for (const teamId of teamIds) {
				await call(
					client.admin.event_teams.banned({ event_id: eventId, team_id: teamId }),
					"封禁战队",
				);
			}
		},
		onSuccess: (_result, teamIds) => {
			toast.success(`已封禁 ${teamIds.length} 支战队`);
			setSelectedIds([]);
			invalidate();
		},
		onError: (error) => toast.apiError("封禁战队失败", error),
	});

	const unban = useMutation({
		mutationFn: async (teamIds: string[]) => {
			for (const teamId of teamIds) {
				await call(
					client.admin.event_teams.unbanned({ event_id: eventId, team_id: teamId }),
					"解除封禁",
				);
			}
		},
		onSuccess: (_result, teamIds) => {
			toast.success(`已解除 ${teamIds.length} 支战队的封禁`);
			setSelectedIds([]);
			invalidate();
		},
		onError: (error) => toast.apiError("解除封禁失败", error),
	});

	const remove = useMutation({
		mutationFn: (ids: string[]) =>
			call<number>(client.admin.event_teams.remove(eventId)(ids), "删除战队"),
		onSuccess: (count) => {
			toast.success(`已删除 ${count} 支战队`);
			setSelectedIds([]);
			invalidate();
		},
		onError: (error) => toast.apiError("删除战队失败", error),
	});

	async function confirmBan(row: TeamResult) {
		const ok = await confirm({
			title: `封禁战队「${row.team.name}」？`,
			description: "封禁在本赛事范围内生效。",
			consequences: [
				"该战队成员在本赛事提交 flag 会被后端拒绝（Team is banned from this event）",
				"该战队从赛事积分榜中移除（后端按 event_teams.banned = false 过滤）",
				"AWD 赛事：该战队的 WireGuard / GameBox 网络会被防火墙规则阻断",
				"战队、成员与解题记录不会被删除；可随时「解除封禁」恢复",
			],
			tone: "danger",
			confirmText: "封禁",
			confirmPhrase: row.team.name,
		});
		if (!ok) return;
		ban.mutate([row.team.id]);
	}

	async function confirmRemove(rows: TeamResult[]) {
		if (rows.length === 0) return;
		const single = rows.length === 1 ? rows[0] : null;
		const ok = await confirm({
			title: single ? `删除战队「${single.team.name}」？` : `删除所选 ${rows.length} 支战队？`,
			description: "删除战队会同时清理其成员的赛事参赛记录。",
			consequences: [
				"战队行与战队成员名册（event_team_members）被删除",
				"战队成员在本赛事的参赛记录（event_users）一并移除，积分榜不再显示",
				"该战队的解题记录随 team_id 外键级联删除，成绩不可恢复",
				"如为 AWD 赛事，请确认该战队运行时已无需保留",
			],
			tone: "danger",
			confirmText: "删除战队",
			confirmPhrase: single ? single.team.name : "delete",
		});
		if (!ok) return;
		remove.mutate(rows.map((row) => row.team.id));
	}

	const columns: DataTableColumn<TeamResult>[] = [
		{
			id: "name",
			header: "战队",
			cell: (row) => (
				<div className="min-w-0">
					<span className="font-medium">{row.team.name}</span>
					{row.team.description ? (
						<p className="mt-0.5 line-clamp-1 text-xs text-muted-foreground">{row.team.description}</p>
					) : null}
				</div>
			),
			sortValue: (row) => row.team.name,
		},
		{
			id: "captain",
			header: "队长",
			cell: (row) =>
				row.captain ? <MonoText>{row.captain}</MonoText> : <span className="text-xs">—</span>,
			sortValue: (row) => row.captain,
			hideBelow: "md",
		},
		{
			id: "members",
			header: "成员",
			cell: (row) => <MemberList members={row.members} />,
		},
		{
			id: "points",
			header: "积分",
			align: "right",
			cell: (row) => <MonoText>{row.team.points.toFixed(2)}</MonoText>,
			sortValue: (row) => row.team.points,
		},
		{
			id: "banned",
			header: "状态",
			cell: (row) => (
				<BooleanPill
					value={row.team.banned}
					trueText="已封禁"
					falseText="正常"
					trueTone="danger"
					falseTone="success"
				/>
			),
			sortValue: (row) => (row.team.banned ? 1 : 0),
		},
		{
			id: "created_at",
			header: "创建时间",
			cell: (row) => <MonoText>{formatDateTime(row.team.created_at, { seconds: true })}</MonoText>,
			sortValue: (row) => row.team.created_at,
			hideBelow: "lg",
		},
	];

	const rowActions = (row: TeamResult): ReactNode => (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button variant="ghost" size="icon-sm" aria-label={`${row.team.name} 的操作`}>
					<MoreHorizontal />
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end">
				<DropdownMenuLabel className="max-w-56 truncate">{row.team.name}</DropdownMenuLabel>
				{row.team.banned ? (
					<DropdownMenuItem onSelect={() => unban.mutate([row.team.id])}>
						<ShieldCheck /> 解除封禁
					</DropdownMenuItem>
				) : (
					<DropdownMenuItem variant="destructive" onSelect={() => void confirmBan(row)}>
						<Ban /> 封禁战队
					</DropdownMenuItem>
				)}
				<DropdownMenuSeparator />
				<DropdownMenuItem variant="destructive" onSelect={() => void confirmRemove([row])}>
					<Trash2 /> 删除战队
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	);

	const mobileCard = (row: TeamResult): ReactNode => (
		<MobileFacts
			items={[
				{ k: "战队", v: <span className="font-medium">{row.team.name}</span> },
				{ k: "队长", v: row.captain ? <MonoText>{row.captain}</MonoText> : "—" },
				{ k: "成员", v: <MemberList members={row.members} /> },
				{ k: "积分", v: <MonoText>{row.team.points.toFixed(2)}</MonoText> },
				{
					k: "状态",
					v: (
						<BooleanPill
							value={row.team.banned}
							trueText="已封禁"
							falseText="正常"
							trueTone="danger"
							falseTone="success"
						/>
					),
				},
			]}
		/>
	);

	const selectedRows = (query.data?.items ?? []).filter((item) => selectedIds.includes(item.team.id));

	return (
		<SectionCard
			title="赛事战队"
			description={
				participantMode === "team"
					? "本赛事为战队赛；封禁按战队生效，成员名单来自后端的 event_team_members。"
					: "本赛事为个人赛（participant_mode=individual），通常没有战队数据。"
			}
			actions={
				<Toolbar>
					{selectedIds.length > 0 ? (
						<>
							<Button
								variant="outline"
								size="sm"
								onClick={() => unban.mutate(selectedRows.map((row) => row.team.id))}
							>
								<ShieldCheck /> 解除封禁
							</Button>
							<Button
								variant="destructive"
								size="sm"
								onClick={() => {
									void (async () => {
										const ok = await confirm({
											title: `封禁所选 ${selectedRows.length} 支战队？`,
											description: "封禁在本赛事范围内生效。",
											consequences: [
												"这些战队成员在本赛事提交 flag 会被后端拒绝",
												"这些战队会从赛事积分榜中移除",
												"战队与解题记录保留，可随时解除封禁",
											],
											tone: "danger",
											confirmText: "全部封禁",
											confirmPhrase: "ban",
										});
										if (!ok) return;
										ban.mutate(selectedRows.map((row) => row.team.id));
									})();
								}}
							>
								<Ban /> 封禁所选
							</Button>
							<Button
								variant="destructive"
								size="sm"
								onClick={() => void confirmRemove(selectedRows)}
							>
								<Trash2 /> 删除所选
							</Button>
						</>
					) : null}
				</Toolbar>
			}
		>
			<PagedTable
				query={pagedQuery}
				columns={columns}
				getRowId={(row) => row.team.id}
				page={safePage}
				pageSize={pageSize}
				total={total}
				onPageChange={setPage}
				onPageSizeChange={(size) => {
					setPageSize(size);
					setPage(1);
				}}
				rowActions={rowActions}
				selectable
				selectedIds={selectedIds}
				onSelectionChange={setSelectedIds}
				mobileCard={mobileCard}
				errorTitle="加载赛事战队失败"
				empty={
					<EmptyBlock
						title={participantMode === "team" ? "还没有战队" : "个人赛没有战队"}
						description={
							participantMode === "team"
								? "战队由选手端创建 / 平台分配，管理端只能查看、封禁或删除。"
								: "本赛事 participant_mode=individual，后端不会返回战队数据。"
						}
						icon={<Users className="size-5" />}
					/>
				}
			/>
		</SectionCard>
	);
}
