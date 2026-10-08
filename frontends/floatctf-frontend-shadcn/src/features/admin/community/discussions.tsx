/**
 * 管理端 · 讨论治理（`/admin/community/discussions`）。
 *
 * 真实接口：`client.admin.discussions.fetch/get/remove/getComments/removeComment`。
 * 后端语义（`apps/api/src/modules/community/discussion/{admin.rs,dto.rs}`）：
 * - 列表返回 `DiscussionWithAuthor`（`#[serde(flatten)]` 展平）—— 除 `Discussions`
 *   字段外还带 `author_nickname` / `author_avatar` / `is_liked`；
 * - 列表 `filter` 只映射 `id` / `title` / `author_id`；
 * - 详情 `get` 只返回讨论本体（不含作者）；评论列表支持 `page` / `limit`。
 */

import { useEffect, useState, type ReactNode } from "react";
import {
	keepPreviousData,
	useMutation,
	useQuery,
	useQueryClient,
} from "@tanstack/react-query";
import { Eye, MessageSquare, MoreHorizontal, RefreshCw, Trash2 } from "lucide-react";

import type { DiscussionComments, Discussions } from "@floatctf/sdk/entity";

import { call, callList, callVoid } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { MarkdownView } from "~/components/app/markdown";
import {
	CopyText,
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
	LoadingBlock,
	QueryState,
	RefreshingBadge,
	TableSkeleton,
} from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { AbsoluteTime, RelativeTime, UserAvatar } from "~/components/app/user-cell";
import { Button } from "~/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "~/components/ui/dropdown-menu";
import {
	Sheet,
	SheetContent,
	SheetDescription,
	SheetHeader,
	SheetTitle,
} from "~/components/ui/sheet";
import { Separator } from "~/components/ui/separator";
import { formatInt, truncate } from "~/lib/format";
import { useDocumentTitle } from "~/lib/hooks";

import {
	DeleteSelectedButton,
	SearchInput,
	sanitizeFilterValue,
	useListState,
} from "./components";

const PAGE_SIZE_OPTIONS = [10, 20, 50];

/** 后端 `DiscussionWithAuthor` 的展平形态（SDK 类型只声明了 `Discussions`）。 */
type DiscussionRow = Discussions & {
	author_nickname?: string;
	author_avatar?: string | null;
	is_liked?: boolean;
};

export function AdminDiscussionsPage(): ReactNode {
	useDocumentTitle("讨论治理 · FloatCTF 控制台");
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const list = useListState(20);
	const [selected, setSelected] = useState<string[]>([]);
	const [openId, setOpenId] = useState<string | null>(null);

	const filter = list.filter;
	// 后端 filter 只映射 id/title/author_id：正文搜索不可用，这里只搜标题。
	const keyword = sanitizeFilterValue(filter);
	const remoteFilter = keyword.length === 0 ? undefined : `title:${keyword}`;

	useEffect(() => {
		setSelected([]);
	}, [list.page, filter]);

	const query = useQuery({
		queryKey: qk.admin.discussions({
			page: list.page,
			limit: list.pageSize,
			...(remoteFilter ? { filter: remoteFilter } : {}),
		}),
		queryFn: () =>
			callList<DiscussionRow>(
				client.admin.discussions.fetch({
					page: list.page,
					limit: list.pageSize,
					...(remoteFilter ? { filter: remoteFilter } : {}),
				}),
			),
		placeholderData: keepPreviousData,
	});

	const removeMutation = useMutation({
		mutationFn: (ids: string[]) =>
			callVoid(client.admin.discussions.remove(ids), "删除讨论"),
		onSuccess: (_data, ids) => {
			toast.success(`已删除 ${ids.length} 条讨论`);
			setSelected([]);
			void queryClient.invalidateQueries({ queryKey: qk.admin.discussions() });
			void queryClient.invalidateQueries({ queryKey: qk.discussions.all });
		},
		onError: (error) => toast.apiError("删除讨论失败", error),
	});

	async function removeRows(rows: DiscussionRow[]) {
		if (rows.length === 0) return;
		const ok = await confirm({
			title: rows.length === 1 ? "删除这条讨论？" : `删除 ${rows.length} 条讨论？`,
			description:
				rows.length === 1
					? rows[0].title
					: `包含：${rows.slice(0, 3).map((row) => row.title).join("、")}${
							rows.length > 3 ? ` 等 ${rows.length} 条` : ""
						}`,
			consequences: [
				"讨论正文与其下的全部评论会一并删除，选手端无法再访问",
				"删除后不可恢复",
			],
			tone: "danger",
			confirmText: "删除",
		});
		if (!ok) return;
		removeMutation.mutate(rows.map((row) => row.id));
	}

	const columns: DataTableColumn<DiscussionRow>[] = [
		{
			id: "title",
			header: "标题",
			sortValue: (row) => row.title,
			cell: (row) => <span className="font-medium">{row.title}</span>,
		},
		{
			id: "author",
			header: "作者",
			sortValue: (row) => row.author_nickname ?? row.author_id,
			cell: (row) => (
				<div className="flex items-center gap-2">
					<UserAvatar
						name={row.author_nickname ?? row.author_id}
						avatar={row.author_avatar ?? undefined}
						size="sm"
					/>
					<span className="truncate">{row.author_nickname ?? row.author_id}</span>
				</div>
			),
		},
		{
			id: "counts",
			header: "互动",
			hideBelow: "md",
			cell: (row) => (
				<span className="tnum font-mono text-xs text-muted-foreground">
					浏览 {formatInt(row.view_count)} · 赞 {formatInt(row.like_count)} · 评论{" "}
					{formatInt(row.comment_count)}
				</span>
			),
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
				title="讨论"
				description="讨论列表只读；可删除讨论与其中的评论。点标题打开详情抽屉。"
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
					</Toolbar>
				}
			/>

			<SectionCard
				title="讨论列表"
				description="按更新时间倒序；搜索仅作用于标题（后端 filter 不支持正文）。"
				actions={
					<div className="flex flex-wrap items-center gap-2">
						<SearchInput
							value={list.search}
							onChange={list.setSearch}
							placeholder="搜索标题"
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
					errorTitle="加载讨论失败"
					isEmpty={(result) => result.items.length === 0}
					empty={
						<EmptyBlock
							variant={filter.length > 0 ? "filtered" : "empty"}
							title={filter.length > 0 ? "没有匹配的讨论" : "暂无讨论"}
							description={
								filter.length > 0 ? "试试调整搜索关键字。" : "选手发布讨论后会出现在这里。"
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
								onRowClick={(row) => setOpenId(row.id)}
								rowActions={(row) => (
									<DropdownMenu>
										<DropdownMenuTrigger asChild>
											<Button variant="ghost" size="icon-sm" aria-label="讨论操作">
												<MoreHorizontal />
											</Button>
										</DropdownMenuTrigger>
										<DropdownMenuContent align="end">
											<DropdownMenuItem onSelect={() => setOpenId(row.id)}>
												<Eye />
												查看详情
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
										<p className="flex items-center gap-2 text-sm text-muted-foreground">
											<UserAvatar
												name={row.author_nickname ?? row.author_id}
												avatar={row.author_avatar ?? undefined}
												size="sm"
											/>
											{row.author_nickname ?? row.author_id}
										</p>
										<p className="tnum font-mono text-xs text-muted-foreground">
											浏览 {formatInt(row.view_count)} · 评论 {formatInt(row.comment_count)}
										</p>
										<div className="flex items-center gap-1">
											<Button variant="ghost" size="sm" onClick={() => setOpenId(row.id)}>
												<Eye />
												查看详情
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

			<DiscussionDetailSheet discussionId={openId} onOpenChange={(open) => !open && setOpenId(null)} />
		</PageBody>
	);
}

/** 讨论详情抽屉：正文用 `<MarkdownView>` 渲染，并列出评论供逐条治理。 */
function DiscussionDetailSheet({
	discussionId,
	onOpenChange,
}: {
	discussionId: string | null;
	onOpenChange: (open: boolean) => void;
}): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();

	const detail = useQuery({
		queryKey: qk.admin.discussion(discussionId ?? ""),
		queryFn: () => call<Discussions>(client.admin.discussions.get(discussionId ?? ""), "讨论详情"),
		enabled: discussionId !== null,
	});

	const comments = useQuery({
		queryKey: qk.admin.discussionComments(discussionId ?? ""),
		queryFn: () =>
			callList<DiscussionComments>(
				client.admin.discussions.getComments(discussionId ?? "", { page: 1, limit: 100 }),
			),
		enabled: discussionId !== null,
	});

	const removeComment = useMutation({
		mutationFn: (commentId: string) =>
			callVoid(
				client.admin.discussions.removeComment(discussionId ?? "", commentId),
				"删除评论",
			),
		onSuccess: () => {
			toast.success("评论已删除");
			void queryClient.invalidateQueries({
				queryKey: qk.admin.discussionComments(discussionId ?? ""),
			});
			void queryClient.invalidateQueries({ queryKey: qk.admin.discussions() });
		},
		onError: (error) => toast.apiError("删除评论失败", error),
	});

	async function deleteComment(comment: DiscussionComments) {
		const ok = await confirm({
			title: "删除这条评论？",
			description: truncate(comment.content, 120),
			consequences: ["该评论会从讨论详情中永久移除", "评论数会同步减少，删除后不可恢复"],
			tone: "danger",
			confirmText: "删除评论",
		});
		if (!ok) return;
		removeComment.mutate(comment.id);
	}

	return (
		<Sheet open={discussionId !== null} onOpenChange={onOpenChange}>
			<SheetContent className="flex w-full flex-col gap-0 overflow-y-auto sm:max-w-3xl">
				<SheetHeader>
					<SheetTitle>{detail.data?.title ?? "讨论详情"}</SheetTitle>
					<SheetDescription>
						{discussionId ? (
							<span className="inline-flex items-center gap-2">
								ID <CopyText value={discussionId} label="讨论 ID" />
							</span>
						) : null}
					</SheetDescription>
				</SheetHeader>

				<div className="space-y-4 px-4 pb-6">
					{detail.isPending ? (
						<LoadingBlock label="加载讨论详情…" />
					) : detail.isError ? (
						<ErrorBlock
							error={detail.error}
							title="加载讨论详情失败"
							onRetry={() => void detail.refetch()}
						/>
					) : detail.data ? (
						<>
							<div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
								<TonePill tone="muted">
									作者 {detail.data.author_id}
								</TonePill>
								<span>浏览 {formatInt(detail.data.view_count)}</span>
								<span>点赞 {formatInt(detail.data.like_count)}</span>
								<span>评论 {formatInt(detail.data.comment_count)}</span>
							</div>
							<div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
								<span>
									创建 <AbsoluteTime value={detail.data.created_at} seconds />
								</span>
								<span>
									更新 <RelativeTime value={detail.data.updated_at} />
								</span>
							</div>
							<Separator />
							<MarkdownView>{detail.data.content}</MarkdownView>
						</>
					) : null}

					<Separator />

					<div className="space-y-2">
						<div className="flex items-center justify-between gap-2">
							<h3 className="flex items-center gap-2 text-sm font-medium">
								<MessageSquare className="size-4" />
								评论
								{comments.data ? (
									<MonoText className="text-muted-foreground">
										{formatInt(comments.data.meta.total ?? comments.data.items.length)}
									</MonoText>
								) : null}
							</h3>
							<Button
								variant="outline"
								size="sm"
								onClick={() => void comments.refetch()}
								disabled={comments.isFetching}
							>
								<RefreshCw />
								刷新评论
							</Button>
						</div>

						{comments.isPending ? (
							<LoadingBlock label="加载评论…" />
						) : comments.isError ? (
							<ErrorBlock
								error={comments.error}
								title="加载评论失败"
								onRetry={() => void comments.refetch()}
							/>
						) : comments.data.items.length === 0 ? (
							<EmptyBlock title="暂无评论" description="这条讨论还没有人回复。" />
						) : (
							<ul className="divide-y">
								{comments.data.items.map((comment) => (
									<li key={comment.id} className="flex items-start justify-between gap-3 py-3">
										<div className="min-w-0 space-y-1">
											<p className="text-xs text-muted-foreground">
												<MonoText>{comment.author_id}</MonoText>
												<span className="ml-2">
													<AbsoluteTime value={comment.created_at} seconds />
												</span>
											</p>
											<p className="text-sm break-words whitespace-pre-wrap">
												{comment.content}
											</p>
										</div>
										<Button
											variant="ghost"
											size="icon-sm"
											className="text-destructive"
											aria-label="删除评论"
											title="删除评论"
											disabled={removeComment.isPending}
											onClick={() => void deleteComment(comment)}
										>
											<Trash2 />
										</Button>
									</li>
								))}
							</ul>
						)}
						{comments.data && (comments.data.meta.total ?? 0) > comments.data.items.length ? (
							<p className="text-xs text-muted-foreground">
								仅显示最近 {comments.data.items.length} 条评论（共{" "}
								{formatInt(comments.data.meta.total)} 条）。
							</p>
						) : null}
					</div>
				</div>
			</SheetContent>
		</Sheet>
	);
}
