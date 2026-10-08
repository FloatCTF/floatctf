/**
 * 赛事控制台 · 题目标签（jeopardy_event_challenges）。
 *
 * 覆盖：列表（柯里化 `fetch(eventId)({page,limit})`）、从题库勾选加入（`add`）、
 * 批量设分（`setPoints`）、发布 / 隐藏（`open` / `hidden`）、移除（柯里化 `remove(eventId)`）。
 */

import { useMemo, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Eye, EyeOff, MoreHorizontal, Plus, Target, Trash2 } from "lucide-react";

import { call, callList } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { BooleanPill, TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { Field, FormFooter, FormSheet } from "~/components/app/form";
import { MonoText, SectionCard, Toolbar } from "~/components/app/page";
import { EmptyBlock, InlineError, TableSkeleton } from "~/components/app/states";
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
import { Input } from "~/components/ui/input";
import type { AdminEventChallengeResult, ChallengesListItem } from "@floatctf/sdk";
import { formatDateTime } from "~/lib/format";
import { useDebouncedValue } from "~/lib/hooks";

import { MobileFacts, PagedTable } from "./shared";

function buildStatusTone(status: string | undefined): "success" | "warning" | "muted" {
	if (status === "ready") return "success";
	if (!status) return "muted";
	return "warning";
}

export function EventChallengesTab({ eventId }: { eventId: string }): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(20);
	const [selectedIds, setSelectedIds] = useState<string[]>([]);
	const [addOpen, setAddOpen] = useState(false);
	const [pointsTarget, setPointsTarget] = useState<string[] | null>(null);

	const query = useQuery({
		queryKey: qk.admin.eventChallenges(eventId, { page, limit: pageSize }),
		queryFn: () =>
			callList<AdminEventChallengeResult>(
				client.admin.event_challenges.fetch(eventId)({ page, limit: pageSize }),
			),
	});

	function invalidate() {
		void queryClient.invalidateQueries({ queryKey: qk.admin.eventChallenges(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.admin.event(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.admin.eventLogs(eventId) });
	}

	const openChallenges = useMutation({
		mutationFn: (ids: string[]) =>
			call(client.admin.event_challenges.open({ event_id: eventId, challenge_id_list: ids }), "发布题目"),
		onSuccess: (_result, ids) => {
			toast.success(`已发布 ${ids.length} 道题目`, "选手端立即可见（若赛事本身可见）。");
			setSelectedIds([]);
			invalidate();
		},
		onError: (error) => toast.apiError("发布题目失败", error),
	});

	const hideChallenges = useMutation({
		mutationFn: (ids: string[]) =>
			call(
				client.admin.event_challenges.hidden({ event_id: eventId, challenge_id_list: ids }),
				"隐藏题目",
			),
		onSuccess: (_result, ids) => {
			toast.success(`已隐藏 ${ids.length} 道题目`);
			setSelectedIds([]);
			invalidate();
		},
		onError: (error) => toast.apiError("隐藏题目失败", error),
	});

	const removeChallenges = useMutation({
		mutationFn: (ids: string[]) =>
			call<number>(client.admin.event_challenges.remove(eventId)(ids), "移除题目"),
		onSuccess: (count) => {
			toast.success(`已从赛事移除 ${count} 道题目`);
			setSelectedIds([]);
			invalidate();
		},
		onError: (error) => toast.apiError("移除题目失败", error),
	});

	const setPoints = useMutation({
		mutationFn: (input: { ids: string[]; points: number }) =>
			call(
				client.admin.event_challenges.setPoints({
					event_id: eventId,
					challenge_id_list: input.ids,
					points: input.points,
				}),
				"设置分值",
			),
		onSuccess: () => {
			toast.success("分值已更新", "之后的解出按新分值结算，已得分不追溯调整。");
			setPointsTarget(null);
			invalidate();
		},
		onError: (error) => toast.apiError("设置分值失败", error),
	});

	async function confirmRemove(rows: AdminEventChallengeResult[]) {
		if (rows.length === 0) return;
		const single = rows.length === 1 ? rows[0] : null;
		const ok = await confirm({
			title: single
				? `从赛事移除「${single.challenge.name}」？`
				: `从赛事移除所选 ${rows.length} 道题目？`,
			description: "仅解除「赛事 ↔ 题目」的挂载关系。",
			consequences: [
				"该题目从赛事题目列表移除，选手端立即不再显示（题目本身仍留在题库中）",
				"已产生的解题记录（jeopardy_challenge_solves）不会被删除",
				"如需重新参赛，需再次从题库把题目加入本赛事（分值需重设）",
			],
			tone: "danger",
			confirmText: "移除",
		});
		if (!ok) return;
		removeChallenges.mutate(rows.map((row) => row.challenge.id));
	}

	const columns: DataTableColumn<AdminEventChallengeResult>[] = [
		{
			id: "challenge_id",
			header: "题目 ID",
			cell: (row) => <MonoText>{row.challenge.id}</MonoText>,
			sortValue: (row) => row.challenge.id,
			hideBelow: "lg",
		},
		{
			id: "name",
			header: "题目名称",
			cell: (row) => <span className="font-medium">{row.challenge.name}</span>,
			sortValue: (row) => row.challenge.name,
		},
		{
			id: "category",
			header: "分类",
			cell: (row) => <TonePill tone="neutral">{row.challenge.category}</TonePill>,
			sortValue: (row) => row.challenge.category,
		},
		{
			id: "points",
			header: "分值",
			align: "right",
			cell: (row) => <MonoText>{row.event_challenge.points.toFixed(2)}</MonoText>,
			sortValue: (row) => row.event_challenge.points,
		},
		{
			id: "hidden",
			header: "发布状态",
			cell: (row) => (
				<BooleanPill
					value={row.event_challenge.hidden}
					trueText="已隐藏"
					falseText="已发布"
					trueTone="muted"
					falseTone="success"
				/>
			),
			sortValue: (row) => (row.event_challenge.hidden ? 1 : 0),
		},
		{
			id: "build_status",
			header: "包状态",
			cell: (row) => (
				<TonePill tone={buildStatusTone(row.challenge.build_status)}>
					{row.challenge.build_status ?? "未知"}
				</TonePill>
			),
			sortValue: (row) => row.challenge.build_status ?? "",
			hideBelow: "lg",
		},
	];

	const rowActions = (row: AdminEventChallengeResult): ReactNode => (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button variant="ghost" size="icon-sm" aria-label={`${row.challenge.name} 的操作`}>
					<MoreHorizontal />
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end">
				<DropdownMenuLabel className="max-w-56 truncate">{row.challenge.name}</DropdownMenuLabel>
				<DropdownMenuItem onSelect={() => setPointsTarget([row.challenge.id])}>
					<Target /> 设置分值
				</DropdownMenuItem>
				{row.event_challenge.hidden ? (
					<DropdownMenuItem onSelect={() => openChallenges.mutate([row.challenge.id])}>
						<Eye /> 发布到选手端
					</DropdownMenuItem>
				) : (
					<DropdownMenuItem onSelect={() => hideChallenges.mutate([row.challenge.id])}>
						<EyeOff /> 从选手端隐藏
					</DropdownMenuItem>
				)}
				<DropdownMenuSeparator />
				<DropdownMenuItem variant="destructive" onSelect={() => void confirmRemove([row])}>
					<Trash2 /> 从赛事移除
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	);

	const mobileCard = (row: AdminEventChallengeResult): ReactNode => (
		<MobileFacts
			items={[
				{ k: "题目", v: <span className="font-medium">{row.challenge.name}</span> },
				{ k: "分类", v: <TonePill tone="neutral">{row.challenge.category}</TonePill> },
				{ k: "分值", v: <MonoText>{row.event_challenge.points.toFixed(2)}</MonoText> },
				{
					k: "状态",
					v: (
						<BooleanPill
							value={row.event_challenge.hidden}
							trueText="已隐藏"
							falseText="已发布"
							trueTone="muted"
							falseTone="success"
						/>
					),
				},
			]}
		/>
	);

	return (
		<SectionCard
			title="赛事题目"
			description="挂载自题库的题目；隐藏的题目不会出现在选手端（发布状态逐题控制）。"
			actions={
				<Toolbar>
					{selectedIds.length > 0 ? (
						<>
							<Button
								variant="outline"
								size="sm"
								disabled={openChallenges.isPending}
								onClick={() => openChallenges.mutate(selectedIds)}
							>
								<Eye /> 发布所选（{selectedIds.length}）
							</Button>
							<Button
								variant="outline"
								size="sm"
								disabled={hideChallenges.isPending}
								onClick={() => hideChallenges.mutate(selectedIds)}
							>
								<EyeOff /> 隐藏所选
							</Button>
							<Button
								variant="outline"
								size="sm"
								onClick={() => setPointsTarget(selectedIds)}
							>
								<Target /> 批量设分
							</Button>
							<Button
								variant="destructive"
								size="sm"
								onClick={() => {
									const rows = (query.data?.items ?? []).filter((item) =>
										selectedIds.includes(item.challenge.id),
									);
									void confirmRemove(rows);
								}}
							>
								<Trash2 /> 移除所选
							</Button>
						</>
					) : null}
					<Button size="sm" onClick={() => setAddOpen(true)}>
						<Plus /> 添加题目
					</Button>
				</Toolbar>
			}
		>
			<PagedTable
				query={query}
				columns={columns}
				getRowId={(row) => row.challenge.id}
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
				errorTitle="加载赛事题目失败"
				empty={
					<EmptyBlock
						title="赛事还没有题目"
						description="点「添加题目」从题库勾选要挂到本赛事的题目。"
						action={
							<Button size="sm" onClick={() => setAddOpen(true)}>
								<Plus /> 添加题目
							</Button>
						}
					/>
				}
			/>

			{addOpen ? (
				<AddChallengesSheet eventId={eventId} onClose={() => setAddOpen(false)} onDone={invalidate} />
			) : null}

			{pointsTarget ? (
				<SetPointsSheet
					count={pointsTarget.length}
					isPending={setPoints.isPending}
					onClose={() => setPointsTarget(null)}
					onSubmit={(points) => setPoints.mutate({ ids: pointsTarget, points })}
				/>
			) : null}
		</SectionCard>
	);
}

/* ── 从题库添加 ─────────────────────────────────────────────────────────── */

function AddChallengesSheet({
	eventId,
	onClose,
	onDone,
}: {
	eventId: string;
	onClose: () => void;
	onDone: () => void;
}): ReactNode {
	const client = useClient();
	const [search, setSearch] = useState("");
	const [page, setPage] = useState(1);
	const [pageSize] = useState(10);
	const [selectedIds, setSelectedIds] = useState<string[]>([]);
	const [points, setPoints] = useState("");
	const keyword = useDebouncedValue(search, 300);

	const filter = useMemo(() => {
		const cleaned = keyword.replace(/[&|]/g, " ").trim();
		return cleaned === "" ? undefined : `name:${cleaned}`;
	}, [keyword]);

	const query = useQuery({
		queryKey: qk.admin.challenges({ page, limit: pageSize, filter }),
		queryFn: () =>
			callList<ChallengesListItem>(
				client.admin.challenges.fetch({ page, limit: pageSize, filter }),
			),
	});

	const parsedPoints = points.trim() === "" ? undefined : Number(points);
	const pointsValid =
		parsedPoints === undefined ||
		(Number.isFinite(parsedPoints) && parsedPoints > 0 && parsedPoints <= 1_000_000);

	const add = useMutation({
		mutationFn: () =>
			call(
				client.admin.event_challenges.add({
					event_id: eventId,
					challenge_id_list: selectedIds,
					points: parsedPoints,
				}),
				"添加题目",
			),
		onSuccess: () => {
			toast.success(`已添加 ${selectedIds.length} 道题目`);
			onDone();
			onClose();
		},
		onError: (error) => toast.apiError("添加题目失败", error),
	});

	const columns: DataTableColumn<ChallengesListItem>[] = [
		{ id: "name", header: "题目名称", cell: (row) => row.name, sortValue: (row) => row.name },
		{
			id: "category",
			header: "分类",
			cell: (row) => <TonePill tone="neutral">{row.category}</TonePill>,
			sortValue: (row) => row.category,
		},
		{
			id: "build_status",
			header: "包状态",
			cell: (row) => (
				<TonePill tone={buildStatusTone(row.build_status)}>{row.build_status ?? "未知"}</TonePill>
			),
			sortValue: (row) => row.build_status ?? "",
		},
		{
			id: "updated_at",
			header: "更新时间",
			cell: (row) => <MonoText>{formatDateTime(row.updated_at)}</MonoText>,
			sortValue: (row) => row.updated_at,
			hideBelow: "md",
		},
	];

	return (
		<FormSheet
			open
			onOpenChange={(next) => {
				if (!next) onClose();
			}}
			width="xl"
			title="从题库添加题目"
			description="只有 build_status = ready 的题目可以被加入赛事（后端会拒绝未就绪的包）。分值留空则后端默认 100。"
			footer={
				<FormFooter
					formId="add-challenges-form"
					onCancel={onClose}
					submitLabel={`添加${selectedIds.length > 0 ? `（${selectedIds.length}）` : ""}`}
					isPending={add.isPending}
					disabled={selectedIds.length === 0 || !pointsValid}
				/>
			}
		>
			<form
				id="add-challenges-form"
				className="space-y-3"
				onSubmit={(submitEvent) => {
					submitEvent.preventDefault();
					if (selectedIds.length === 0 || !pointsValid) return;
					add.mutate();
				}}
			>
				<div className="flex flex-wrap items-end gap-3">
					<Field label="搜索题目" htmlFor="challenge-search" className="min-w-52 flex-1">
						<Input
							id="challenge-search"
							value={search}
							onChange={(changeEvent) => {
								setSearch(changeEvent.target.value);
								setPage(1);
							}}
							placeholder="按名称过滤…"
						/>
					</Field>
					<Field
						label="分值（可选）"
						htmlFor="challenge-points"
						hint="1 ~ 1000000，留空 = 100"
						error={pointsValid ? undefined : "分值必须是不大于 1000000 的正数"}
					>
						<Input
							id="challenge-points"
							type="number"
							min={1}
							value={points}
							className="w-40"
							onChange={(changeEvent) => setPoints(changeEvent.target.value)}
						/>
					</Field>
				</div>

				{query.isPending ? (
					<TableSkeleton rows={6} columns={4} />
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
										{ k: "名称", v: row.name },
										{ k: "分类", v: <TonePill tone="neutral">{row.category}</TonePill> },
										{
											k: "包状态",
											v: (
												<TonePill tone={buildStatusTone(row.build_status)}>
													{row.build_status ?? "未知"}
												</TonePill>
											),
										},
									]}
								/>
							)}
						/>
						<div className="flex flex-wrap items-center justify-between gap-2">
							<p className="tnum text-xs text-muted-foreground">
								共 {query.data?.meta?.total ?? query.data?.items.length ?? 0} 道题目 · 本页已选{" "}
								{selectedIds.length} 道
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
								<span className="tnum text-xs text-muted-foreground">
									第 {page} / {Math.max(1, Math.ceil((query.data?.meta?.total ?? query.data?.items.length ?? 0) / pageSize))} 页
								</span>
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

/* ── 设置分值 ───────────────────────────────────────────────────────────── */

function SetPointsSheet({
	count,
	isPending,
	onClose,
	onSubmit,
}: {
	count: number;
	isPending: boolean;
	onClose: () => void;
	onSubmit: (points: number) => void;
}): ReactNode {
	const [value, setValue] = useState("100");
	const parsed = Number(value);
	const valid = Number.isFinite(parsed) && parsed > 0 && parsed <= 1_000_000;

	return (
		<FormSheet
			open
			onOpenChange={(next) => {
				if (!next) onClose();
			}}
			title={`设置 ${count} 道题目的分值`}
			description="提交后新解按新分值结算，已获得的分值不追溯调整。"
			footer={
				<FormFooter
					onCancel={onClose}
					submitLabel="保存分值"
					isPending={isPending}
					disabled={!valid}
					formId="set-points-form"
				/>
			}
		>
			<form
				id="set-points-form"
				onSubmit={(submitEvent) => {
					submitEvent.preventDefault();
					if (valid) onSubmit(parsed);
				}}
			>
				<Field
					label="分值"
					htmlFor="set-points-value"
					required
					hint="1 ~ 1000000"
					error={valid ? undefined : "分值必须是不大于 1000000 的正数"}
				>
					<Input
						id="set-points-value"
						type="number"
						min={1}
						value={value}
						onChange={(changeEvent) => setValue(changeEvent.target.value)}
					/>
				</Field>
			</form>
		</FormSheet>
	);
}
