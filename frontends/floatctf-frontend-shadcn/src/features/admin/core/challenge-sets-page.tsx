/**
 * 题集管理（`/admin/challenge-sets`）—— 列表 + Sheet 新建/编辑 + 批量删除。
 *
 * 题目与题集的关联在详情页维护（`/admin/challenge-sets/:setId`）：
 * SDK 的 `createChallengeSet` 只接受 `Partial<ChallengeSets>`（后端其实还接受
 * `challenge_id_list`，但公共类型面没有暴露），因此创建后进入详情页挂题。
 */

import { useState, type ReactNode } from "react";
import { useNavigate } from "react-router";
import { useMutation, useQuery } from "@tanstack/react-query";
import { ArrowRight, Blocks, Pencil, Plus, RefreshCw, Trash2 } from "lucide-react";

import { call, callList } from "~/api/call";
import { useAppQueryClient, useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { Field, FormFooter, FormGrid, FormSheet } from "~/components/app/form";
import {
	MonoText,
	PageBody,
	PageHeader,
	PaginationBar,
	SectionCard,
	Toolbar,
} from "~/components/app/page";
import { EmptyBlock, QueryState, TableSkeleton } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { DropdownMenuItem, DropdownMenuSeparator } from "~/components/ui/dropdown-menu";
import { Input } from "~/components/ui/input";
import { Textarea } from "~/components/ui/textarea";
import { formatDateTime } from "~/lib/format";
import { useDocumentTitle } from "~/lib/hooks";
import type { QueryParams } from "@floatctf/sdk";
import type { ChallengeSets } from "@floatctf/sdk/entity";

import { RowMenu, SearchInput, useAdminListState } from "./shared";

const SEARCH_KEYS = ["name", "description"];

interface SetFormState {
	name: string;
	description: string;
}

export function AdminChallengeSetsPage(): ReactNode {
	useDocumentTitle("题集管理 · FloatCTF");
	const client = useClient();
	const navigate = useNavigate();
	const queryClient = useAppQueryClient();
	const confirm = useConfirm();
	const list = useAdminListState(SEARCH_KEYS);

	const [selectedIds, setSelectedIds] = useState<string[]>([]);
	const [editing, setEditing] = useState<{ mode: "create" | "edit"; row?: ChallengeSets } | null>(null);
	const [form, setForm] = useState<SetFormState>({ name: "", description: "" });
	const [nameError, setNameError] = useState<string | undefined>(undefined);

	const params: QueryParams = {
		page: list.page,
		limit: list.pageSize,
		...(list.filter ? { filter: list.filter } : {}),
	};

	const query = useQuery({
		queryKey: qk.admin.challengeSets(params),
		queryFn: () => callList<ChallengeSets>(client.admin.challenges.getChallengeSets(params)),
	});

	function invalidate(): void {
		void queryClient.invalidateQueries({ queryKey: qk.admin.challengeSets() });
	}

	const save = useMutation({
		mutationFn: (input: { mode: "create" | "edit"; id?: string; body: SetFormState }) => {
			const body = { name: input.body.name.trim(), description: input.body.description };
			if (input.mode === "create") {
				return call<ChallengeSets>(
					client.admin.challenges.createChallengeSet(body),
					"创建题集",
				);
			}
			// patchChallengeSet(Partial<ChallengeSets>) 的 URL 用 `.id` 拼接，**必须**带 id。
			return call<ChallengeSets>(
				client.admin.challenges.patchChallengeSet({ id: input.id ?? "", ...body }),
				"更新题集",
			);
		},
		onSuccess: (_data, input) => {
			toast.success(input.mode === "create" ? "题集已创建" : "题集已更新");
			invalidate();
			setEditing(null);
			setNameError(undefined);
		},
		onError: (error) => toast.apiError("保存题集失败", error),
	});

	const remove = useMutation({
		mutationFn: (ids: string[]) =>
			call<number>(client.admin.challenges.deleteChallengeSet(ids), "删除题集"),
		onSuccess: (count) => {
			toast.success(`已删除 ${count} 个题集`, "题集内的题目本身不受影响。");
			invalidate();
			setSelectedIds([]);
		},
		onError: (error) => toast.apiError("删除题集失败", error),
	});

	function openCreate(): void {
		setForm({ name: "", description: "" });
		setNameError(undefined);
		setEditing({ mode: "create" });
	}

	function openEdit(row: ChallengeSets): void {
		setForm({ name: row.name, description: row.description ?? "" });
		setNameError(undefined);
		setEditing({ mode: "edit", row });
	}

	function submit(): void {
		if (!editing) return;
		if (!form.name.trim()) {
			setNameError("题集名称不能为空");
			toast.warning("请先填写题集名称");
			return;
		}
		setNameError(undefined);
		save.mutate({ mode: editing.mode, id: editing.row?.id, body: form });
	}

	async function confirmRemoveOne(row: ChallengeSets): Promise<void> {
		const ok = await confirm({
			title: `删除题集「${row.name}」？`,
			description: "该操作不可恢复。",
			consequences: [
				"题集与其题目关联（challenge_set_items）一并删除",
				"题目本身保留在题库中，不会被删除",
			],
			tone: "danger",
			confirmText: "删除",
			confirmPhrase: row.name,
		});
		if (ok) remove.mutate([row.id]);
	}

	async function confirmRemoveSelected(): Promise<void> {
		if (selectedIds.length === 0) {
			toast.warning("请先选择要删除的题集");
			return;
		}
		const ok = await confirm({
			title: `删除选中的 ${selectedIds.length} 个题集？`,
			description: "该操作不可恢复。",
			consequences: [
				`${selectedIds.length} 个题集及其题目关联会被删除`,
				"题目本身保留在题库中",
			],
			tone: "danger",
			confirmText: "删除",
			confirmPhrase: "delete",
		});
		if (ok) remove.mutate(selectedIds);
	}

	const columns: DataTableColumn<ChallengeSets>[] = [
		{
			id: "name",
			header: "题集",
			cell: (row) => (
				<div className="min-w-0">
					<p className="truncate text-sm font-medium">{row.name}</p>
					<MonoText className="text-muted-foreground">{row.id}</MonoText>
				</div>
			),
			sortValue: (row) => row.name,
		},
		{
			id: "description",
			header: "描述",
			cell: (row) =>
				row.description ? (
					<span className="line-clamp-2 text-sm text-muted-foreground">{row.description}</span>
				) : (
					<span className="text-xs text-muted-foreground">—</span>
				),
			hideBelow: "sm",
		},
		{
			id: "created_at",
			header: "创建时间",
			cell: (row) => (
				<span className="tnum text-xs text-muted-foreground">{formatDateTime(row.created_at)}</span>
			),
			sortValue: (row) => Date.parse(row.created_at),
			hideBelow: "md",
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

	return (
		<PageBody>
			<PageHeader
				title="题集管理"
				description="题集把题库中的题目按主题组织起来；题目内容与题集无关。"
				actions={
					<Toolbar>
						<Button variant="outline" onClick={() => query.refetch()} disabled={query.isFetching}>
							<RefreshCw className={query.isFetching ? "animate-spin" : undefined} />
							刷新
						</Button>
						<Button onClick={openCreate}>
							<Plus /> 新建题集
						</Button>
					</Toolbar>
				}
			/>

			<SectionCard
				title="题集列表"
				description="点任意一行进入题集详情，在那里增删题目。"
				actions={
					<Toolbar>
						<SearchInput
							value={list.search}
							onChange={list.setSearch}
							placeholder="搜索名称 / 描述"
						/>
						{selectedIds.length > 0 ? (
							<Button
								variant="destructive"
								size="sm"
								onClick={() => void confirmRemoveSelected()}
								disabled={remove.isPending}
							>
								<Trash2 /> 删除选中（{selectedIds.length}）
							</Button>
						) : null}
					</Toolbar>
				}
				contentClassName="p-0"
				footer={
					// 数据到位前不渲染分页条，避免出现「共 0 条」这种与事实不符的文案。
					query.data ? (
						<PaginationBar
							page={list.page}
							pageSize={list.pageSize}
							total={query.data.meta?.total ?? 0}
							onPageChange={list.setPage}
							onPageSizeChange={list.setPageSize}
						/>
					) : null
				}
			>
				<QueryState
					query={query}
					skeleton={<TableSkeleton rows={6} columns={4} />}
					errorTitle="加载题集失败"
					isEmpty={(result) => result.items.length === 0}
					empty={
						list.filter ? (
							<EmptyBlock
								variant="filtered"
								title="没有匹配的题集"
								description={`没有名称 / 描述匹配「${list.search}」的题集。`}
							/>
						) : (
							<EmptyBlock
								title="还没有题集"
								description="创建题集后可以往里挂题目。"
								action={
									<Button size="sm" onClick={openCreate}>
										<Plus /> 新建题集
									</Button>
								}
							/>
						)
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
							onRowClick={(row) => void navigate(`/admin/challenge-sets/${row.id}`)}
							mobileCard={(row) => (
								<div className="space-y-1.5">
									<p className="text-sm font-medium">{row.name}</p>
									<p className="text-xs text-muted-foreground">{row.description || "无描述"}</p>
									<div className="flex flex-wrap gap-2 pt-1">
										<Button
											variant="outline"
											size="sm"
											onClick={() => void navigate(`/admin/challenge-sets/${row.id}`)}
										>
											<Blocks /> 管理题目
										</Button>
										<Button variant="outline" size="sm" onClick={() => openEdit(row)}>
											<Pencil /> 编辑
										</Button>
										<Button
											variant="destructive"
											size="sm"
											onClick={() => void confirmRemoveOne(row)}
										>
											<Trash2 /> 删除
										</Button>
									</div>
								</div>
							)}
							rowActions={(row) => (
								<RowMenu label={`题集 ${row.name} 的操作`}>
									<DropdownMenuItem
										onSelect={() => void navigate(`/admin/challenge-sets/${row.id}`)}
									>
										<Blocks /> 管理题目
									</DropdownMenuItem>
									<DropdownMenuItem onSelect={() => openEdit(row)}>
										<Pencil /> 编辑
									</DropdownMenuItem>
									<DropdownMenuSeparator />
									<DropdownMenuItem variant="destructive" onSelect={() => void confirmRemoveOne(row)}>
										<Trash2 /> 删除
									</DropdownMenuItem>
								</RowMenu>
							)}
						/>
					)}
				</QueryState>
			</SectionCard>

			<FormSheet
				open={editing !== null}
				onOpenChange={(open) => {
					if (!open) setEditing(null);
				}}
				title={editing?.mode === "edit" ? "编辑题集" : "新建题集"}
				description={
					editing?.mode === "edit"
						? "修改题集名称与描述；题目关联在详情页维护。"
						: "创建后进入详情页，从题库里选择题目加入。"
				}
				width="md"
				footer={
					<FormFooter
						onCancel={() => setEditing(null)}
						submitLabel={editing?.mode === "edit" ? "保存修改" : "创建题集"}
						isPending={save.isPending}
						formId="admin-set-form"
					/>
				}
			>
				<form
					id="admin-set-form"
					className="space-y-4"
					onSubmit={(event) => {
						event.preventDefault();
						submit();
					}}
				>
					<FormGrid columns={1}>
						<Field label="题集名称" htmlFor="set-name" required error={nameError}>
							<Input
								id="set-name"
								value={form.name}
								onChange={(event) => setForm({ ...form, name: event.target.value })}
							/>
						</Field>
						<Field label="描述" htmlFor="set-description" hint="可选，用于说明题集主题。">
							<Textarea
								id="set-description"
								rows={3}
								value={form.description}
								onChange={(event) => setForm({ ...form, description: event.target.value })}
							/>
						</Field>
					</FormGrid>
					{editing?.mode === "create" ? (
						<p className="flex items-center gap-1.5 text-xs text-muted-foreground">
							<ArrowRight className="size-3.5" />
							创建完成后请到详情页添加题目。
						</p>
					) : null}
				</form>
			</FormSheet>
		</PageBody>
	);
}
