/**
 * 赛事控制台 · 成员标签（event_users）。
 *
 * `fetch` / `delete` 是柯里化方法（`fetch(eventId)({page,limit})`、`delete(eventId)(id_list)`）；
 * `banned` / `unbanned` 是普通方法。`add` 运行时返回 AxiosResponse —— 用 `call` 助手解包即可。
 */

import { useMemo, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Ban, MoreHorizontal, Plus, ShieldCheck, Trash2, UserMinus } from "lucide-react";

import { call, callList } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { BooleanPill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { Field, FormFooter, FormSheet } from "~/components/app/form";
import { MonoText, SectionCard, Toolbar } from "~/components/app/page";
import { EmptyBlock, InlineError, TableSkeleton } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { UserCell } from "~/components/app/user-cell";
import { Button } from "~/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "~/components/ui/dropdown-menu";
import { Input } from "~/components/ui/input";
import type { EventUserResult } from "@floatctf/sdk";
import type { Users } from "@floatctf/sdk/entity";
import { formatDateTime } from "~/lib/format";
import { useDebouncedValue } from "~/lib/hooks";

import { MobileFacts, PagedTable } from "./shared";

function displayName(user: Users): string {
	return user.nickname && user.nickname.length > 0 ? user.nickname : user.username;
}

export function EventMembersTab({
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
	const [addOpen, setAddOpen] = useState(false);

	const query = useQuery({
		queryKey: qk.admin.eventUsers(eventId, { page, limit: pageSize }),
		queryFn: () =>
			callList<EventUserResult>(client.admin.event_users.fetch(eventId)({ page, limit: pageSize })),
	});

	function invalidate() {
		void queryClient.invalidateQueries({ queryKey: qk.admin.eventUsers(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.admin.eventTeams(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.admin.event(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.admin.eventLogs(eventId) });
	}

	// 后端没有批量封禁接口：逐条串行调用，成功后统一提示与失效（避免 N 次 toast / refetch）。
	const ban = useMutation({
		mutationFn: async (userIds: string[]) => {
			for (const userId of userIds) {
				await call(
					client.admin.event_users.banned({ event_id: eventId, user_id: userId }),
					"封禁成员",
				);
			}
		},
		onSuccess: (_result, userIds) => {
			toast.success(`已封禁 ${userIds.length} 名成员`);
			setSelectedIds([]);
			invalidate();
		},
		onError: (error) => toast.apiError("封禁失败", error),
	});

	const unban = useMutation({
		mutationFn: async (userIds: string[]) => {
			for (const userId of userIds) {
				await call(
					client.admin.event_users.unbanned({ event_id: eventId, user_id: userId }),
					"解除封禁",
				);
			}
		},
		onSuccess: (_result, userIds) => {
			toast.success(`已解除 ${userIds.length} 名成员的封禁`);
			setSelectedIds([]);
			invalidate();
		},
		onError: (error) => toast.apiError("解除封禁失败", error),
	});

	const remove = useMutation({
		mutationFn: (ids: string[]) =>
			call<number>(client.admin.event_users.delete(eventId)(ids), "移出赛事"),
		onSuccess: (count) => {
			toast.success(`已将 ${count} 名成员移出赛事`);
			setSelectedIds([]);
			invalidate();
		},
		onError: (error) => toast.apiError("移出成员失败", error),
	});

	async function confirmBan(row: EventUserResult) {
		const name = displayName(row.user);
		const ok = await confirm({
			title: `封禁「${name}」？`,
			description: "封禁在本赛事范围内生效，不影响该账号在其他赛事的使用。",
			consequences: [
				"该选手在本赛事提交 flag 会被后端拒绝（User is banned from this event）",
				"该选手从赛事积分榜中移除（后端按 event_users.banned = false 过滤）",
				"账号本身、参赛记录与已提交的解题记录不会被删除；可随时「解除封禁」恢复",
			],
			tone: "danger",
			confirmText: "封禁",
			confirmPhrase: row.user.username,
		});
		if (!ok) return;
		ban.mutate([row.user.id]);
	}

	async function confirmRemove(rows: EventUserResult[]) {
		if (rows.length === 0) return;
		const single = rows.length === 1 ? rows[0] : null;
		const ok = await confirm({
			title: single ? `将「${displayName(single.user)}」移出赛事？` : `将所选 ${rows.length} 名成员移出赛事？`,
			description: "移除的是赛事参赛记录（event_users），不是平台账号。",
			consequences: [
				"该选手在本赛事的参赛记录被删除，赛事积分榜不再显示该选手（积分榜以 event_users 为准）",
				"平台账号与其解题记录不会被删除",
				"如需重新参赛需再次添加；重新加入后此前的积分不会自动恢复显示",
			],
			tone: "danger",
			confirmText: "移出",
		});
		if (!ok) return;
		remove.mutate(rows.map((row) => row.user.id));
	}

	const columns: DataTableColumn<EventUserResult>[] = [
		{
			id: "user",
			header: "选手",
			cell: (row) => (
				<UserCell
					username={row.user.username}
					nickname={row.user.nickname}
					avatar={row.user.avatar}
				/>
			),
			sortValue: (row) => row.user.nickname || row.user.username,
		},
		{
			id: "points",
			header: "赛事积分",
			align: "right",
			cell: (row) => <MonoText>{row.event_user.points.toFixed(2)}</MonoText>,
			sortValue: (row) => row.event_user.points,
		},
		{
			id: "banned",
			header: "状态",
			cell: (row) => (
				<BooleanPill
					value={row.event_user.banned}
					trueText="已封禁"
					falseText="正常"
					trueTone="danger"
					falseTone="success"
				/>
			),
			sortValue: (row) => (row.event_user.banned ? 1 : 0),
		},
		{
			id: "joined_at",
			header: "加入时间",
			cell: (row) => <MonoText>{formatDateTime(row.event_user.joined_at, { seconds: true })}</MonoText>,
			sortValue: (row) => row.event_user.joined_at,
			hideBelow: "md",
		},
	];

	const rowActions = (row: EventUserResult): ReactNode => (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button variant="ghost" size="icon-sm" aria-label={`${displayName(row.user)} 的操作`}>
					<MoreHorizontal />
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end">
				<DropdownMenuLabel className="max-w-56 truncate">{displayName(row.user)}</DropdownMenuLabel>
				{row.event_user.banned ? (
					<DropdownMenuItem onSelect={() => unban.mutate([row.user.id])}>
						<ShieldCheck /> 解除封禁
					</DropdownMenuItem>
				) : (
					<DropdownMenuItem variant="destructive" onSelect={() => void confirmBan(row)}>
						<Ban /> 封禁
					</DropdownMenuItem>
				)}
				<DropdownMenuSeparator />
				{participantMode === "team" ? (
					<DropdownMenuItem disabled>
						<UserMinus /> 战队赛请通过「战队」标签管理成员
					</DropdownMenuItem>
				) : (
					<DropdownMenuItem variant="destructive" onSelect={() => void confirmRemove([row])}>
						<UserMinus /> 移出赛事
					</DropdownMenuItem>
				)}
			</DropdownMenuContent>
		</DropdownMenu>
	);

	const mobileCard = (row: EventUserResult): ReactNode => (
		<MobileFacts
			items={[
				{
					k: "选手",
					v: (
						<UserCell
							username={row.user.username}
							nickname={row.user.nickname}
							avatar={row.user.avatar}
						/>
					),
				},
				{ k: "积分", v: <MonoText>{row.event_user.points.toFixed(2)}</MonoText> },
				{
					k: "状态",
					v: (
						<BooleanPill
							value={row.event_user.banned}
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

	const selectedRows = (query.data?.items ?? []).filter((item) => selectedIds.includes(item.user.id));

	return (
		<SectionCard
			title="赛事成员"
			description={
				participantMode === "team"
					? "战队赛：这里是赛事的参赛账号池；成员的战队归属请在「战队」标签查看。"
					: "个人赛：这里的成员即赛事参赛者，可在此封禁 / 移出。"
			}
			actions={
				<Toolbar>
					{selectedIds.length > 0 ? (
						<>
							<Button
								variant="outline"
								size="sm"
								disabled={unban.isPending}
								onClick={() => unban.mutate(selectedRows.map((row) => row.user.id))}
							>
								<ShieldCheck /> 解除封禁
							</Button>
							<Button
								variant="destructive"
								size="sm"
								disabled={ban.isPending}
								onClick={() => {
									// 批量封禁沿用单条确认词（逐条输入用户名）不可行，这里统一确认一次。
									void (async () => {
										const ok = await confirm({
											title: `封禁所选 ${selectedRows.length} 名成员？`,
											description: "封禁在本赛事范围内生效。",
											consequences: [
												"这些选手在本赛事提交 flag 会被后端拒绝",
												"他们会从赛事积分榜中移除（后端按 banned=false 过滤）",
												"账号与解题记录保留，可随时解除封禁",
											],
											tone: "danger",
											confirmText: "全部封禁",
											confirmPhrase: "ban",
										});
										if (!ok) return;
										ban.mutate(selectedRows.map((row) => row.user.id));
									})();
								}}
							>
								<Ban /> 封禁所选
							</Button>
							{participantMode === "team" ? null : (
								<Button
									variant="destructive"
									size="sm"
									disabled={remove.isPending}
									onClick={() => void confirmRemove(selectedRows)}
								>
									<Trash2 /> 移出所选
								</Button>
							)}
						</>
					) : null}
					<Button size="sm" onClick={() => setAddOpen(true)}>
						<Plus /> 添加成员
					</Button>
				</Toolbar>
			}
		>
			<PagedTable
				query={query}
				columns={columns}
				getRowId={(row) => row.user.id}
				page={page}
				pageSize={pageSize}
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
				errorTitle="加载赛事成员失败"
				empty={
					<EmptyBlock
						title="赛事还没有成员"
						description="点「添加成员」从平台账号里勾选参赛者。"
						action={
							<Button size="sm" onClick={() => setAddOpen(true)}>
								<Plus /> 添加成员
							</Button>
						}
					/>
				}
			/>

			{addOpen ? (
				<AddMembersSheet
					eventId={eventId}
					participantMode={participantMode}
					onClose={() => setAddOpen(false)}
					onDone={invalidate}
				/>
			) : null}
		</SectionCard>
	);
}

/* ── 添加成员 ───────────────────────────────────────────────────────────── */

function AddMembersSheet({
	eventId,
	participantMode,
	onClose,
	onDone,
}: {
	eventId: string;
	participantMode: string;
	onClose: () => void;
	onDone: () => void;
}): ReactNode {
	const client = useClient();
	const [search, setSearch] = useState("");
	const [page, setPage] = useState(1);
	const [pageSize] = useState(10);
	const [selectedIds, setSelectedIds] = useState<string[]>([]);
	const keyword = useDebouncedValue(search, 300);

	const filter = useMemo(() => {
		const cleaned = keyword.replace(/[&|]/g, " ").trim();
		return cleaned === "" ? undefined : `nickname:${cleaned}`;
	}, [keyword]);

	const query = useQuery({
		queryKey: qk.admin.users({ page, limit: pageSize, filter }),
		queryFn: () => callList<Users>(client.admin.users.fetch({ page, limit: pageSize, filter })),
	});

	const add = useMutation({
		mutationFn: () =>
			call(client.admin.event_users.add({ event_id: eventId, user_id_list: selectedIds }), "添加成员"),
		onSuccess: () => {
			toast.success(`已添加 ${selectedIds.length} 名成员`);
			onDone();
			onClose();
		},
		onError: (error) => toast.apiError("添加成员失败", error),
	});

	const columns: DataTableColumn<Users>[] = [
		{
			id: "username",
			header: "用户名",
			cell: (row) => <MonoText>{row.username}</MonoText>,
			sortValue: (row) => row.username,
		},
		{ id: "nickname", header: "昵称", cell: (row) => row.nickname, sortValue: (row) => row.nickname },
		{
			id: "created_at",
			header: "注册时间",
			cell: (row) => <MonoText>{formatDateTime(row.created_at)}</MonoText>,
			sortValue: (row) => row.created_at,
			hideBelow: "md",
		},
	];

	return (
		<FormSheet
			open
			onOpenChange={(next) => {
				if (!next) onClose();
			}}
			width="lg"
			title="添加赛事成员"
			description={
				participantMode === "team"
					? "战队赛：这里只把账号加入赛事参赛池，不会自动加入任何战队（战队归属由队长 / 平台管理）。"
					: "从平台账号中勾选参赛者；重复添加会被后端拒绝。"
			}
			footer={
				<FormFooter
					formId="add-members-form"
					onCancel={onClose}
					submitLabel={`添加${selectedIds.length > 0 ? `（${selectedIds.length}）` : ""}`}
					isPending={add.isPending}
					disabled={selectedIds.length === 0}
				/>
			}
		>
			<form
				id="add-members-form"
				className="space-y-3"
				onSubmit={(submitEvent) => {
					submitEvent.preventDefault();
					if (selectedIds.length === 0) return;
					add.mutate();
				}}
			>
				<Field label="搜索账号" htmlFor="member-search" hint="按昵称过滤（后端 filter 键 nickname）。">
					<Input
						id="member-search"
						value={search}
						onChange={(changeEvent) => {
							setSearch(changeEvent.target.value);
							setPage(1);
						}}
						placeholder="昵称关键字…"
					/>
				</Field>

				{query.isPending ? (
					<TableSkeleton rows={6} columns={3} />
				) : query.isError ? (
					<InlineError error={query.error} />
				) : (
					<>
						<DataTable
							data={query.data?.items ?? []}
							columns={columns}
							getRowId={(row) => row.id}
							selectable
							selectedIds={selectedIds}
							onSelectionChange={setSelectedIds}
							mobileCard={(row) => (
								<MobileFacts
									items={[
										{ k: "用户名", v: <MonoText>{row.username}</MonoText> },
										{ k: "昵称", v: row.nickname },
									]}
								/>
							)}
						/>
						<div className="flex flex-wrap items-center justify-between gap-2">
							<p className="tnum text-xs text-muted-foreground">
								共 {query.data?.meta?.total ?? query.data?.items.length ?? 0} 个账号 · 本页已选{" "}
								{selectedIds.length} 个
							</p>
							<div className="flex items-center gap-2">
								<Button
									type="button"
									variant="outline"
									size="sm"
									disabled={page <= 1}
									onClick={() => setPage((current) => Math.max(1, current - 1))}
								>
									上一页
								</Button>
								<span className="tnum text-xs text-muted-foreground">第 {page} 页</span>
								<Button
									type="button"
									variant="outline"
									size="sm"
									disabled={
										query.data !== undefined &&
										page * pageSize >= (query.data.meta?.total ?? query.data.items.length)
									}
									onClick={() => setPage((current) => current + 1)}
								>
									下一页
								</Button>
							</div>
						</div>
					</>
				)}
			</form>
		</FormSheet>
	);
}
