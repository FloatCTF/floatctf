/**
 * 题集详情（`/admin/challenge-sets/:setId`）—— 题集信息 + 题目增删。
 *
 * 真实接口：
 * - 题集信息：`getChallengeSets({ filter: "id:<setId>", limit: 1 })`
 *   （没有「按 id 取题集」的单条接口，复用列表接口的 `id` 过滤）；
 * - 题集内题目：`getChallengeSet(setId)({ page, limit })` —— **柯里化，必须二次调用**；
 * - 增：`addChallengeToSet({ set_id, challenge_id_list })`；
 * - 删：`removeChallengeFromSet(setId)(id_list)` —— 同上柯里化。
 *
 * 注意：`GET /challenge_sets/{id}` 在服务端只支持 `page` / `limit`，**没有** filter 映射，
 * 因此本页不提供搜索框（避免给出「看起来生效、其实被后端忽略」的假过滤）。
 * 需要找题请用「添加题目」抽屉里的搜索（那走的是 `/challenges`，有完整 filter 映射）。
 */

import { useState, type ReactNode } from "react";
import { Link, useParams } from "react-router";
import { useMutation, useQuery } from "@tanstack/react-query";
import { ArrowLeft, Minus, Plus, RefreshCw } from "lucide-react";

import { call, callList } from "~/api/call";
import { useAppQueryClient, useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { FormSheet } from "~/components/app/form";
import {
	KeyValueList,
	MonoText,
	PageBody,
	PageHeader,
	PaginationBar,
	SectionCard,
	Toolbar,
} from "~/components/app/page";
import {
	EmptyBlock,
	ErrorBlock,
	NotFoundBlock,
	QueryState,
	TableSkeleton,
} from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { formatDateTime } from "~/lib/format";
import { useDocumentTitle } from "~/lib/hooks";
import type { ChallengesListItem, QueryParams } from "@floatctf/sdk";
import type { ChallengeSets } from "@floatctf/sdk/entity";

import { BuildStatusPill, SearchInput, useAdminListState } from "./shared";

export function AdminChallengeSetDetailPage(): ReactNode {
	const { setId = "" } = useParams<{ setId: string }>();
	const client = useClient();
	const queryClient = useAppQueryClient();
	const confirm = useConfirm();

	const metaParams: QueryParams = { filter: `id:${setId}`, limit: 1 };
	const setMeta = useQuery({
		queryKey: qk.admin.challengeSets(metaParams),
		queryFn: () => callList<ChallengeSets>(client.admin.challenges.getChallengeSets(metaParams)),
		enabled: setId !== "",
	});

	const meta = setMeta.data?.items[0] ?? null;
	useDocumentTitle(meta ? `${meta.name} · 题集管理` : "题集详情 · FloatCTF");

	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(20);
	const [selectedIds, setSelectedIds] = useState<string[]>([]);

	const itemsParams: QueryParams = { page, limit: pageSize };
	const items = useQuery({
		queryKey: [...qk.admin.challengeSet(setId), itemsParams],
		// getChallengeSet 是柯里化的：getChallengeSet(id) 返回函数，必须二次调用。
		queryFn: () => callList<ChallengesListItem>(client.admin.challenges.getChallengeSet(setId)(itemsParams)),
		enabled: setId !== "",
	});

	// ── 添加题目（抽屉里的题库选择器）──
	const [pickerOpen, setPickerOpen] = useState(false);
	const [picked, setPicked] = useState<string[]>([]);
	const picker = useAdminListState(["name", "safe_name", "category"]);
	const pickerParams: QueryParams = {
		page: picker.page,
		limit: picker.pageSize,
		...(picker.filter ? { filter: picker.filter } : {}),
	};
	const pickerQuery = useQuery({
		queryKey: qk.admin.challenges(pickerParams),
		queryFn: () => callList<ChallengesListItem>(client.admin.challenges.fetch(pickerParams)),
		enabled: pickerOpen,
	});

	function invalidateSet(): void {
		void queryClient.invalidateQueries({ queryKey: qk.admin.challengeSet(setId) });
		void queryClient.invalidateQueries({ queryKey: qk.admin.challengeSets() });
	}

	const addChallenges = useMutation({
		mutationFn: (ids: string[]) =>
			call<null>(
				client.admin.challenges.addChallengeToSet({ set_id: setId, challenge_id_list: ids }),
				"添加题目",
			),
		onSuccess: (_data, ids) => {
			toast.success(`已向题集添加 ${ids.length} 道题目`);
			invalidateSet();
			setPicked([]);
			setPickerOpen(false);
		},
		onError: (error) => toast.apiError("添加题目失败", error),
	});

	const removeChallenges = useMutation({
		mutationFn: (ids: string[]) =>
			call<number>(client.admin.challenges.removeChallengeFromSet(setId)(ids), "移出题集"),
		onSuccess: (count) => {
			toast.success(`已从题集移出 ${count} 道题目`, "题目本身仍保留在题库中。");
			invalidateSet();
			setSelectedIds([]);
		},
		onError: (error) => toast.apiError("移出题集失败", error),
	});

	async function confirmRemoveOne(row: ChallengesListItem): Promise<void> {
		const ok = await confirm({
			title: `把「${row.name}」移出该题集？`,
			description: "只解除题集关联，不删除题目。",
			consequences: [
				"题目仍保留在题库中，选手端其他入口不受影响",
				"该题在本题集中的顺序信息一并删除，重新添加需再次选择",
			],
			tone: "danger",
			confirmText: "移出",
			confirmPhrase: row.name,
		});
		if (ok) removeChallenges.mutate([row.id]);
	}

	async function confirmRemoveSelected(): Promise<void> {
		if (selectedIds.length === 0) {
			toast.warning("请先选择要移出的题目");
			return;
		}
		const ok = await confirm({
			title: `把选中的 ${selectedIds.length} 道题目移出题集？`,
			description: "只解除题集关联，不删除题目。",
			consequences: [
				`${selectedIds.length} 条题集关联（challenge_set_items）会被删除`,
				"题目、解题记录与实例都不受影响",
			],
			tone: "danger",
			confirmText: "移出",
			confirmPhrase: "delete",
		});
		if (ok) removeChallenges.mutate(selectedIds);
	}

	const columns: DataTableColumn<ChallengesListItem>[] = [
		{
			id: "name",
			header: "题目",
			cell: (row) => (
				<div className="min-w-0">
					<p className="truncate text-sm font-medium">{row.name}</p>
					<MonoText className="text-muted-foreground">{row.safe_name}</MonoText>
				</div>
			),
			sortValue: (row) => row.name,
		},
		{
			id: "category",
			header: "分类",
			cell: (row) => <span className="text-sm">{row.category}</span>,
			sortValue: (row) => row.category,
			hideBelow: "sm",
		},
		{
			id: "version",
			header: "版本 / 构建",
			cell: (row) => (
				<div className="flex flex-wrap items-center gap-1">
					<MonoText>{row.version ?? "—"}</MonoText>
					<BuildStatusPill status={row.build_status} />
				</div>
			),
			hideBelow: "md",
		},
		{
			id: "hidden",
			header: "可见性",
			cell: (row) => (
				<TonePill tone={row.hidden ? "muted" : "success"}>
					{row.hidden ? "已隐藏" : "公开"}
				</TonePill>
			),
			sortValue: (row) => (row.hidden ? 1 : 0),
		},
		{
			id: "updated_at",
			header: "最近更新",
			cell: (row) => (
				<span className="tnum text-xs text-muted-foreground">{formatDateTime(row.updated_at)}</span>
			),
			sortValue: (row) => Date.parse(row.updated_at),
			hideBelow: "lg",
		},
	];

	const backLink = (
		<Link to="/admin/challenge-sets" className="inline-flex items-center gap-1 hover:text-foreground">
			<ArrowLeft className="size-3" /> 返回题集列表
		</Link>
	);

	return (
		<PageBody>
			<PageHeader
				breadcrumbs={backLink}
				title={meta?.name ?? "题集详情"}
				description={meta?.description || "题集内的题目列表；题目本身仍属于题库。"}
				badge={meta ? <MonoText className="text-muted-foreground">{meta.id}</MonoText> : undefined}
				actions={
					<Toolbar>
						<Button variant="outline" onClick={() => items.refetch()} disabled={items.isFetching}>
							<RefreshCw className={items.isFetching ? "animate-spin" : undefined} />
							刷新
						</Button>
						<Button
							onClick={() => {
								setPicked([]);
								setPickerOpen(true);
							}}
						>
							<Plus /> 添加题目
						</Button>
					</Toolbar>
				}
			/>

			{setMeta.isError ? (
				<ErrorBlock
					error={setMeta.error}
					title="加载题集信息失败"
					onRetry={() => setMeta.refetch()}
				/>
			) : null}

			{meta ? (
				<SectionCard title="题集信息">
					<KeyValueList
						columns={2}
						items={[
							{ key: "名称", value: meta.name },
							{
								key: "题目数量",
								value: <MonoText>{items.data ? (items.data.meta?.total ?? 0) : "—"}</MonoText>,
								hint: "由题集内关联实时统计",
							},
							{ key: "创建时间", value: <MonoText>{formatDateTime(meta.created_at)}</MonoText> },
							{ key: "最近更新", value: <MonoText>{formatDateTime(meta.updated_at)}</MonoText> },
						]}
					/>
				</SectionCard>
			) : null}

			<SectionCard
				title="题集内题目"
				description="该接口（GET /challenge_sets/{id}）在服务端只支持分页，不支持搜索过滤，因此这里不提供搜索框。"
				actions={
					selectedIds.length > 0 ? (
						<Button
							variant="destructive"
							size="sm"
							onClick={() => void confirmRemoveSelected()}
							disabled={removeChallenges.isPending}
						>
							<Minus /> 移出选中（{selectedIds.length}）
						</Button>
					) : null
				}
				contentClassName="p-0"
				footer={
					items.data ? (
						<PaginationBar
							page={page}
							pageSize={pageSize}
							total={items.data.meta?.total ?? 0}
							onPageChange={setPage}
							onPageSizeChange={(size) => {
								setPageSize(size);
								setPage(1);
							}}
						/>
					) : null
				}
			>
				<QueryState
					query={items}
					skeleton={<TableSkeleton rows={6} columns={4} />}
					errorTitle="加载题集内题目失败"
					isEmpty={(result) => result.items.length === 0}
					empty={
						<EmptyBlock
							title="题集里还没有题目"
							description="用右上角「添加题目」从题库中选择并加入。"
							action={
								<Button
									size="sm"
									onClick={() => {
										setPicked([]);
										setPickerOpen(true);
									}}
								>
									<Plus /> 添加题目
								</Button>
							}
						/>
					}
				>
					{(result) => (
						<DataTable
							data={result.items}
							getRowId={(row) => row.id}
							columns={columns}
							selectable
							selectedIds={selectedIds}
							onSelectionChange={setSelectedIds}
							mobileCard={(row) => (
								<div className="space-y-1.5">
									<div className="flex items-start justify-between gap-2">
										<span className="text-sm font-medium">{row.name}</span>
										<BuildStatusPill status={row.build_status} />
									</div>
									<MonoText className="text-muted-foreground">
										{row.safe_name} · {row.category}
									</MonoText>
									<Button
										variant="destructive"
										size="sm"
										className="mt-1"
										onClick={() => void confirmRemoveOne(row)}
									>
										<Minus /> 移出题集
									</Button>
								</div>
							)}
							rowActions={(row) => (
								<Button
									variant="ghost"
									size="sm"
									onClick={() => void confirmRemoveOne(row)}
									aria-label={`把 ${row.name} 移出题集`}
									disabled={removeChallenges.isPending}
								>
									<Minus /> 移出
								</Button>
							)}
						/>
					)}
				</QueryState>
			</SectionCard>

			{setMeta.isSuccess && setMeta.data.items.length === 0 ? (
				<NotFoundBlock
					title="题集不存在"
					description={`没有 id 为 ${setId} 的题集，它可能已被删除。`}
					action={
						<Button asChild size="sm">
							<Link to="/admin/challenge-sets">返回题集列表</Link>
						</Button>
					}
				/>
			) : null}

			<FormSheet
				open={pickerOpen}
				onOpenChange={(open) => {
					if (!open) {
						setPickerOpen(false);
						setPicked([]);
					}
				}}
				title="向题集添加题目"
				description="从题库中选择题目；已加入的题目重复添加会被后端唯一约束拒绝。"
				width="xl"
				footer={
					<div className="flex w-full items-center justify-between gap-2">
						<span className="text-xs text-muted-foreground">
							已选 {picked.length} 道
						</span>
						<div className="flex items-center gap-2">
							<Button variant="outline" onClick={() => setPickerOpen(false)}>
								取消
							</Button>
							<Button
								onClick={() => {
									if (picked.length === 0) {
										toast.warning("请先选择要添加的题目");
										return;
									}
									addChallenges.mutate(picked);
								}}
								disabled={addChallenges.isPending || picked.length === 0}
							>
								<Plus /> 添加选中（{picked.length}）
							</Button>
						</div>
					</div>
				}
			>
				<SearchInput
					value={picker.search}
					onChange={picker.setSearch}
					placeholder="搜索题目名称 / safe_name / 分类"
					className="sm:max-w-md"
				/>
				<QueryState
					query={pickerQuery}
					skeleton={<TableSkeleton rows={5} columns={3} />}
					errorTitle="加载题库失败"
					isEmpty={(result) => result.items.length === 0}
					empty={<EmptyBlock variant="filtered" title="没有匹配的题目" />}
				>
					{(result) => (
						<DataTable
							data={result.items}
							getRowId={(row) => row.id}
							selectable
							selectedIds={picked}
							onSelectionChange={setPicked}
							columns={[
								{
									id: "name",
									header: "题目",
									cell: (row) => (
										<div className="min-w-0">
											<p className="truncate text-sm font-medium">{row.name}</p>
											<MonoText className="text-muted-foreground">{row.safe_name}</MonoText>
										</div>
									),
									sortValue: (row) => row.name,
								},
								{
									id: "category",
									header: "分类",
									cell: (row) => <span className="text-sm">{row.category}</span>,
									sortValue: (row) => row.category,
									hideBelow: "sm",
								},
								{
									id: "build_status",
									header: "构建",
									cell: (row) => <BuildStatusPill status={row.build_status} />,
									hideBelow: "sm",
								},
							]}
							mobileCard={(row) => (
								<div className="space-y-1">
									<div className="flex items-center justify-between gap-2">
										<span className="text-sm font-medium">{row.name}</span>
										<BuildStatusPill status={row.build_status} />
									</div>
									<MonoText className="text-muted-foreground">{row.safe_name}</MonoText>
									<p className="text-xs text-muted-foreground">{row.category}</p>
								</div>
							)}
						/>
					)}
				</QueryState>
				{pickerQuery.data ? (
					<PaginationBar
						page={picker.page}
						pageSize={picker.pageSize}
						total={pickerQuery.data.meta?.total ?? 0}
						onPageChange={picker.setPage}
						onPageSizeChange={picker.setPageSize}
					/>
				) : null}
			</FormSheet>
		</PageBody>
	);
}
