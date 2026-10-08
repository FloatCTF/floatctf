/**
 * 管理端 · 全局公告治理（`/admin/community/announcements`）。
 *
 * 真实接口：`client.admin.announcements.fetch/create/patch/remove`。
 * 后端语义（`apps/api/src/modules/platform/announcements/api.rs`）：
 * - 列表支持 `page` / `limit` 服务端分页，`filter` 只映射 `id` / `title` / `content`；
 * - `patch` 走 `PATCH /announcements/{id}`，SDK 要求必须带 `.id`；
 * - 批量删除是 `DELETE` + body `{ id_list }`。
 */

import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import {
	keepPreviousData,
	useMutation,
	useQuery,
	useQueryClient,
} from "@tanstack/react-query";
import { MoreHorizontal, Pencil, Plus, RefreshCw, Trash2 } from "lucide-react";

import type { Announcements } from "@floatctf/sdk/entity";

import { call, callList, callVoid } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { Field, FormFooter, FormSheet } from "~/components/app/form";
import {
	MonoText,
	PageBody,
	PageHeader,
	PaginationBar,
	SectionCard,
	Toolbar,
} from "~/components/app/page";
import {
	EmptyBlock,
	QueryState,
	RefreshingBadge,
	TableSkeleton,
} from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { AbsoluteTime } from "~/components/app/user-cell";
import { Button } from "~/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "~/components/ui/dropdown-menu";
import { Input } from "~/components/ui/input";
import { Textarea } from "~/components/ui/textarea";
import { truncate } from "~/lib/format";
import { useDocumentTitle } from "~/lib/hooks";

import {
	DeleteSelectedButton,
	SearchInput,
	sanitizeFilterValue,
	useListState,
} from "./components";

const PAGE_SIZE_OPTIONS = [10, 20, 50];

export function AdminAnnouncementsPage(): ReactNode {
	useDocumentTitle("公告治理 · FloatCTF 控制台");
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const list = useListState(20);
	const [selected, setSelected] = useState<string[]>([]);
	/** `undefined` = 关闭；`null` = 新建；否则为编辑目标。 */
	const [editing, setEditing] = useState<Announcements | null | undefined>(undefined);

	// 后端 filter 只映射 title/content，多字段用 `|` 做 OR（搜索词先剔除逻辑分隔符）。
	const filter = list.filter;
	const keyword = sanitizeFilterValue(filter);
	const remoteFilter =
		keyword.length === 0 ? undefined : `title:${keyword} | content:${keyword}`;

	useEffect(() => {
		setSelected([]);
	}, [list.page, filter]);

	const query = useQuery({
		queryKey: qk.admin.announcements({
			page: list.page,
			limit: list.pageSize,
			...(remoteFilter ? { filter: remoteFilter } : {}),
		}),
		queryFn: () =>
			callList<Announcements>(
				client.admin.announcements.fetch({
					page: list.page,
					limit: list.pageSize,
					...(remoteFilter ? { filter: remoteFilter } : {}),
				}),
			),
		placeholderData: keepPreviousData,
	});

	const removeMutation = useMutation({
		mutationFn: (ids: string[]) =>
			callVoid(client.admin.announcements.remove(ids), "删除公告"),
		onSuccess: (_data, ids) => {
			toast.success(`已删除 ${ids.length} 条公告`);
			setSelected([]);
			void queryClient.invalidateQueries({ queryKey: qk.admin.announcements() });
			void queryClient.invalidateQueries({ queryKey: qk.announcements.all });
		},
		onError: (error) => toast.apiError("删除公告失败", error),
	});

	async function removeRows(rows: Announcements[]) {
		if (rows.length === 0) return;
		const ok = await confirm({
			title: rows.length === 1 ? "删除这条公告？" : `删除 ${rows.length} 条公告？`,
			description:
				rows.length === 1
					? rows[0].title
					: `包含：${rows.slice(0, 3).map((row) => row.title).join("、")}${
							rows.length > 3 ? ` 等 ${rows.length} 条` : ""
						}`,
			consequences: [
				"选手端公告列表会立即移除这些内容，影响所有已登录用户",
				"删除后不可恢复（公告正文不会保留）",
			],
			tone: "danger",
			confirmText: "删除",
		});
		if (!ok) return;
		removeMutation.mutate(rows.map((row) => row.id));
	}

	const columns: DataTableColumn<Announcements>[] = [
		{
			id: "title",
			header: "标题",
			sortValue: (row) => row.title,
			cell: (row) => <span className="font-medium">{row.title}</span>,
		},
		{
			id: "content",
			header: "内容",
			hideBelow: "md",
			cell: (row) => (
				<span className="text-muted-foreground">{truncate(row.content ?? "", 60) || "—"}</span>
			),
		},
		{
			id: "publisher",
			header: "发布者",
			hideBelow: "sm",
			sortValue: (row) => row.publisher,
			cell: (row) => <span>{row.publisher}</span>,
		},
		{
			id: "updated_at",
			header: "更新时间",
			align: "right",
			sortValue: (row) => Date.parse(row.updated_at),
			cell: (row) => <AbsoluteTime value={row.updated_at} />,
		},
	];

	return (
		<PageBody>
			<PageHeader
				title="公告"
				description="全站公告的创建、修订与删除。公告对所有用户可见。"
				actions={
					<Toolbar>
						<RefreshingBadge active={query.isFetching && !query.isPending} />
						<Button
							variant="outline"
							size="sm"
							onClick={() => void query.refetch()}
							disabled={query.isFetching}
						>
							<RefreshCw />
							刷新
						</Button>
						<Button size="sm" onClick={() => setEditing(null)}>
							<Plus />
							新建公告
						</Button>
					</Toolbar>
				}
			/>

			<SectionCard
				title="公告列表"
				description="搜索命中标题或正文；按更新时间倒序。"
				actions={
					<div className="flex flex-wrap items-center gap-2">
						<SearchInput
							value={list.search}
							onChange={list.setSearch}
							placeholder="搜索标题 / 正文"
						/>
						<DeleteSelectedButton
							count={selected.length}
							isPending={removeMutation.isPending}
							onClick={() =>
								void removeRows(
									(query.data?.items ?? []).filter((row) => selected.includes(row.id)),
								)
							}
						/>
					</div>
				}
			>
				<QueryState
					query={query}
					skeleton={<TableSkeleton rows={6} columns={4} />}
					errorTitle="加载公告失败"
					isEmpty={(result) => result.items.length === 0}
					empty={
						<EmptyBlock
							variant={filter.length > 0 ? "filtered" : "empty"}
							title={filter.length > 0 ? "没有匹配的公告" : "暂无公告"}
							description={
								filter.length > 0
									? "试试调整搜索关键字。"
									: "新建一条公告后，这里会显示它的发布时间与发布者。"
							}
						/>
					}
				>
					{(result) => (
						<div className="space-y-3">
							<DataTable
								data={result.items}
								getRowId={(row) => row.id}
								columns={columns}
								selectable
								selectedIds={selected}
								onSelectionChange={setSelected}
								rowActions={(row) => (
									<DropdownMenu>
										<DropdownMenuTrigger asChild>
											<Button variant="ghost" size="icon-sm" aria-label="公告操作">
												<MoreHorizontal />
											</Button>
										</DropdownMenuTrigger>
										<DropdownMenuContent align="end">
											<DropdownMenuItem onSelect={() => setEditing(row)}>
												<Pencil />
												编辑
											</DropdownMenuItem>
											<DropdownMenuItem
												variant="destructive"
												onSelect={() => void removeRows([row])}
											>
												<Trash2 />
												删除
											</DropdownMenuItem>
										</DropdownMenuContent>
									</DropdownMenu>
								)}
								mobileCard={(row) => (
									<div className="space-y-2">
										<div className="flex items-start justify-between gap-2">
											<span className="font-medium">{row.title}</span>
											<AbsoluteTime value={row.updated_at} />
										</div>
										<p className="text-sm text-muted-foreground">
											{truncate(row.content ?? "", 120) || "（无正文）"}
										</p>
										<div className="flex items-center justify-between gap-2">
											<MonoText className="text-muted-foreground">{row.publisher}</MonoText>
											<div className="flex items-center gap-1">
												<Button variant="ghost" size="sm" onClick={() => setEditing(row)}>
													<Pencil />
													编辑
												</Button>
												<Button
													variant="ghost"
													size="sm"
													className="text-destructive"
													onClick={() => void removeRows([row])}
												>
													<Trash2 />
													删除
												</Button>
											</div>
										</div>
									</div>
								)}
							/>
							<PaginationBar
								page={list.page}
								pageSize={list.pageSize}
								total={result.meta.total ?? result.items.length}
								onPageChange={list.setPage}
								onPageSizeChange={list.setPageSize}
								pageSizeOptions={PAGE_SIZE_OPTIONS}
							/>
						</div>
					)}
				</QueryState>
			</SectionCard>

			<AnnouncementFormSheet
				open={editing !== undefined}
				row={editing ?? null}
				onOpenChange={(open) => {
					if (!open) setEditing(undefined);
				}}
			/>
		</PageBody>
	);
}

function AnnouncementFormSheet({
	open,
	row,
	onOpenChange,
}: {
	open: boolean;
	row: Announcements | null;
	onOpenChange: (open: boolean) => void;
}): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const [title, setTitle] = useState("");
	const [content, setContent] = useState("");
	const [error, setError] = useState<string | null>(null);

	useEffect(() => {
		if (!open) return;
		setTitle(row?.title ?? "");
		setContent(row?.content ?? "");
		setError(null);
	}, [open, row]);

	function invalidate() {
		void queryClient.invalidateQueries({ queryKey: qk.admin.announcements() });
		void queryClient.invalidateQueries({ queryKey: qk.announcements.all });
	}

	const createMutation = useMutation({
		mutationFn: (input: { title: string; content: string }) =>
			call(client.admin.announcements.create(input), "创建公告"),
		onSuccess: () => {
			toast.success("公告已创建");
			invalidate();
			onOpenChange(false);
		},
		onError: (err) => toast.apiError("创建公告失败", err),
	});

	const patchMutation = useMutation({
		mutationFn: (input: { id: string; title: string; content: string }) =>
			call(client.admin.announcements.patch(input), "更新公告"),
		onSuccess: () => {
			toast.success("公告已更新");
			invalidate();
			onOpenChange(false);
		},
		onError: (err) => toast.apiError("更新公告失败", err),
	});

	const isPending = createMutation.isPending || patchMutation.isPending;

	function submit(event: FormEvent<HTMLFormElement>) {
		event.preventDefault();
		const nextTitle = title.trim();
		if (nextTitle.length === 0) {
			setError("标题不能为空");
			return;
		}
		setError(null);
		if (row) patchMutation.mutate({ id: row.id, title: nextTitle, content });
		else createMutation.mutate({ title: nextTitle, content });
	}

	return (
		<FormSheet
			open={open}
			onOpenChange={onOpenChange}
			title={row ? "编辑公告" : "新建公告"}
			description={
				row
					? "保存后所有用户刷新即可看到新内容；发布者与发布时间保持不变。"
					: "公告发布后立即对所有用户可见。"
			}
			width="lg"
			footer={
				<FormFooter
					formId="announcement-form"
					isPending={isPending}
					submitLabel={row ? "保存" : "发布"}
					onCancel={() => onOpenChange(false)}
					hint={row ? `ID ${row.id}` : "标题为必填项"}
				/>
			}
		>
			<form id="announcement-form" className="space-y-4" onSubmit={submit}>
				<Field label="标题" htmlFor="announcement-title" required error={error}>
					<Input
						id="announcement-title"
						value={title}
						onChange={(event) => setTitle(event.target.value)}
						placeholder="一句话说明这次通知"
					/>
				</Field>
				<Field
					label="正文"
					htmlFor="announcement-content"
					hint="纯文本，保留换行（选手端按原文展示，不解析 Markdown）。"
				>
					<Textarea
						id="announcement-content"
						value={content}
						onChange={(event) => setContent(event.target.value)}
						style={{ minHeight: 220 }}
					/>
				</Field>
			</form>
		</FormSheet>
	);
}
