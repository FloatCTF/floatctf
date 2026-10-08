/**
 * 讨论区 —— 列表（新建 / 编辑 / 删除）、我的讨论、详情（评论 CRUD + 点赞）。
 *
 * 业务语义对照 Default Frontend（只作行为参照，未复制其视觉）：
 * - 「我的讨论」用后端真实过滤参数 `filter: author_id:<me.id>`（GET /discussions 支持
 *   `id` / `title` / `author_id` 三个过滤键），不做客户端假过滤。
 * - 过滤串语法来自后端 `build_filter_condition`：**条件之间必须用 `&` 分隔**，
 *   否则后续条件会被并入前一个条件的值。
 * - 删除讨论会级联删除全部评论（`discussion_comments.discussion_id` ON DELETE CASCADE）；
 *   删除评论会级联删除其回复（`parent_id` ON DELETE CASCADE）——确认框里如实写明。
 */

import { useState, type ReactNode } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
	ArrowLeft,
	Eye,
	MessageSquare,
	MessagesSquare,
	Pencil,
	ThumbsUp,
	Trash2,
} from "lucide-react";

import type { DiscussionWithAuthor } from "@floatctf/sdk";
import type { Users } from "@floatctf/sdk/entity";

import { call, callList, callVoid } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { useMe } from "~/auth/store";
import { TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { MarkdownView } from "~/components/app/markdown";
import {
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
import { RelativeTime, UserAvatar, UserCell } from "~/components/app/user-cell";
import { Button } from "~/components/ui/button";
import { useDebouncedValue, useDocumentTitle } from "~/lib/hooks";

import {
	CommentCard,
	CommentComposer,
	DiscussionFormSheet,
	SearchField,
	buildCommentTree,
	type DiscussionComment,
} from "./components";

/** 删除讨论（列表 / 我的 / 详情共用）：`useConfirm` + 真实后果 + 失效全部讨论查询。 */
function useDeleteDiscussion() {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const [pendingId, setPendingId] = useState<string | null>(null);

	const mutation = useMutation({
		mutationFn: (discussionId: string) =>
			callVoid(client.service.discussions.remove(discussionId), "删除讨论"),
		onSuccess: () => {
			toast.success("讨论已删除");
			void queryClient.invalidateQueries({ queryKey: qk.discussions.all });
		},
		onError: (error) => toast.apiError("删除讨论失败", error),
		onSettled: () => setPendingId(null),
	});

	async function requestDelete(row: { id: string; title: string }): Promise<boolean> {
		const ok = await confirm({
			title: "删除该讨论？",
			description: row.title,
			consequences: [
				"该讨论及其全部评论会被删除，不可恢复",
				"其他选手将无法再查看这个帖子",
			],
			tone: "danger",
			confirmText: "删除讨论",
		});
		if (!ok) return false;
		setPendingId(row.id);
		mutation.mutate(row.id);
		return true;
	}

	return { requestDelete, pendingId, isPending: mutation.isPending };
}

/** 列表 / 我的：共用列定义（`mineId` 用于标记「我的」条目）。 */
function discussionColumns(mineId: string | undefined): DataTableColumn<DiscussionWithAuthor>[] {
	return [
		{
			id: "title",
			header: "标题",
			sortValue: (row) => row.title,
			cell: (row) => (
				<div className="flex min-w-0 items-center gap-2">
					<Link
						to={`/community/discussions/${row.id}`}
						className="truncate text-sm font-medium hover:underline"
					>
						{row.title}
					</Link>
					{mineId !== undefined && row.author_id === mineId ? (
						<TonePill tone="info">我的</TonePill>
					) : null}
				</div>
			),
		},
		{
			id: "author",
			header: "作者",
			hideBelow: "sm",
			sortValue: (row) => row.author_nickname,
			cell: (row) => (
				<UserCell
					username={row.author_nickname}
					nickname={row.author_nickname}
					avatar={row.author_avatar ?? null}
					secondary="none"
				/>
			),
		},
		{
			id: "stats",
			header: "互动",
			align: "right",
			cell: (row) => (
				<div className="flex items-center justify-end gap-3 text-xs text-muted-foreground">
					<span className="inline-flex items-center gap-1" title="浏览">
						<Eye className="size-3.5" />
						{row.view_count}
					</span>
					<span className="inline-flex items-center gap-1" title="点赞">
						<ThumbsUp className="size-3.5" />
						{row.like_count}
					</span>
					<span className="inline-flex items-center gap-1" title="评论">
						<MessageSquare className="size-3.5" />
						{row.comment_count}
					</span>
				</div>
			),
		},
		{
			id: "created_at",
			header: "创建",
			hideBelow: "md",
			sortValue: (row) => row.created_at,
			cell: (row) => <RelativeTime value={row.created_at} />,
		},
		{
			id: "updated_at",
			header: "更新",
			hideBelow: "lg",
			sortValue: (row) => row.updated_at,
			cell: (row) => <RelativeTime value={row.updated_at} />,
		},
	];
}

function discussionMobileCard(row: DiscussionWithAuthor, mineId: string | undefined): ReactNode {
	return (
		<div className="space-y-1.5">
			<div className="flex items-start justify-between gap-2">
				<Link
					to={`/community/discussions/${row.id}`}
					className="text-sm font-medium hover:underline"
				>
					{row.title}
				</Link>
				{mineId !== undefined && row.author_id === mineId ? (
					<TonePill tone="info">我的</TonePill>
				) : null}
			</div>
			<div className="flex items-center gap-2">
				<UserAvatar name={row.author_nickname} avatar={row.author_avatar ?? null} size="sm" />
				<span className="truncate text-xs text-muted-foreground">{row.author_nickname}</span>
			</div>
			<div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
				<span>浏览 {row.view_count}</span>
				<span>点赞 {row.like_count}</span>
				<span>评论 {row.comment_count}</span>
				<RelativeTime value={row.updated_at} />
			</div>
		</div>
	);
}

/** 讨论区列表：全站讨论，支持按标题过滤与新建。 */
export function DiscussionsPage(): ReactNode {
	useDocumentTitle("讨论区 · FloatCTF");
	const client = useClient();
	const me = useMe();
	const navigate = useNavigate();
	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(20);
	const [search, setSearch] = useState("");
	const [formOpen, setFormOpen] = useState(false);
	const debouncedSearch = useDebouncedValue(search, 300);
	const deleteAction = useDeleteDiscussion();
	const columns = discussionColumns(me?.id);

	const trimmed = debouncedSearch.trim();
	const filter = trimmed === "" ? undefined : `title:${trimmed}`;

	const query = useQuery({
		queryKey: qk.discussions.list({ page, limit: pageSize, filter }),
		queryFn: () =>
			callList<DiscussionWithAuthor>(
				client.service.discussions.fetch({ page, limit: pageSize, filter }),
			),
	});

	return (
		<PageBody>
			<PageHeader
				title="讨论区"
				description="选手之间的技术交流：解题思路、环境问题、平台反馈。"
				actions={
					<Toolbar>
						<Button variant="outline" asChild>
							<Link to="/community/discussions/mine">
								<MessagesSquare /> 我的讨论
							</Link>
						</Button>
						<Button onClick={() => setFormOpen(true)}>
							<Pencil /> 新建讨论
						</Button>
					</Toolbar>
				}
			/>

			<SectionCard
				title="全部讨论"
				description="按最近更新时间排序，支持按标题搜索。"
				actions={
					<Toolbar>
						<RefreshingBadge active={query.isFetching && !query.isPending} />
						<SearchField
							value={search}
							placeholder="搜索标题…"
							onChange={(value) => {
								setSearch(value);
								setPage(1);
							}}
						/>
					</Toolbar>
				}
				contentClassName="px-0"
				footer={
					<PaginationBar
						page={page}
						pageSize={pageSize}
						total={query.data?.meta.total ?? query.data?.items.length ?? 0}
						onPageChange={setPage}
						onPageSizeChange={(size) => {
							setPageSize(size);
							setPage(1);
						}}
					/>
				}
			>
				<QueryState
					query={query}
					skeleton={<TableSkeleton rows={6} columns={5} />}
					errorTitle="加载讨论失败"
					isEmpty={(result) => result.items.length === 0}
					empty={
						trimmed === "" ? (
							<EmptyBlock
								title="还没有讨论"
								description="发起第一个话题，让其他选手参与进来。"
								icon={<MessagesSquare className="size-5" />}
								action={
									<Button size="sm" onClick={() => setFormOpen(true)}>
										新建讨论
									</Button>
								}
							/>
						) : (
							<EmptyBlock
								variant="filtered"
								title="没有匹配的讨论"
								description={`没有标题包含「${trimmed}」的讨论。`}
								action={
									<Button size="sm" variant="outline" onClick={() => setSearch("")}>
										清除搜索
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
							onRowClick={(row) => void navigate(`/community/discussions/${row.id}`)}
							mobileCard={(row) => discussionMobileCard(row, me?.id)}
							rowActions={(row) => (
								<div className="flex items-center gap-1">
									<Button variant="ghost" size="sm" asChild>
										<Link to={`/community/discussions/${row.id}`}>查看</Link>
									</Button>
									{me !== null && row.author_id === me.id ? (
										<Button
											variant="ghost"
											size="icon-sm"
											aria-label="删除讨论"
											title="删除讨论"
											disabled={deleteAction.isPending}
											onClick={() => void deleteAction.requestDelete(row)}
										>
											<Trash2 className="text-destructive" />
										</Button>
									) : null}
								</div>
							)}
						/>
					)}
				</QueryState>
			</SectionCard>

			<DiscussionFormSheet open={formOpen} onOpenChange={setFormOpen} />
		</PageBody>
	);
}

/** 我的讨论：后端按 `author_id` 过滤（不是客户端过滤），支持发帖 / 编辑 / 删除闭环。 */
export function MyDiscussionsPage(): ReactNode {
	useDocumentTitle("我的讨论 · FloatCTF");
	const client = useClient();
	const me = useMe();
	const navigate = useNavigate();
	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(20);
	const [search, setSearch] = useState("");
	const [formOpen, setFormOpen] = useState(false);
	const [editing, setEditing] = useState<{ id: string; title: string; content: string } | null>(
		null,
	);
	const debouncedSearch = useDebouncedValue(search, 300);
	const deleteAction = useDeleteDiscussion();

	// 本页必须知道当前账号 id 才能构造 `author_id` 过滤串。这里复用会话引导的
	// `qk.auth.me()`（同键共享缓存，不会重复请求），并显式处理它失败的情况，
	// 避免拿不到账号时永远停在加载态。
	const authQuery = useQuery({
		queryKey: qk.auth.me(),
		queryFn: () => call<Users>(client.service.users.getMe(), "当前账号"),
	});
	const meId = me?.id ?? authQuery.data?.id;
	const columns = discussionColumns(meId);

	const trimmed = debouncedSearch.trim();
	// 条件之间用 `&` 分隔（后端解析规则），`author_id` 始终生效。
	const filterParts = [
		...(trimmed === "" ? [] : [`title:${trimmed}`]),
		...(meId ? [`author_id:${meId}`] : []),
	];
	const filter = filterParts.join(" & ");

	const query = useQuery({
		queryKey: qk.discussions.mine({ page, limit: pageSize, filter }),
		queryFn: () =>
			callList<DiscussionWithAuthor>(
				client.service.discussions.fetch({ page, limit: pageSize, filter }),
			),
		enabled: meId !== undefined,
	});

	return (
		<PageBody>
			<PageHeader
				title="我的讨论"
				description="你发布的全部讨论，可在这里编辑标题 / 正文或删除帖子。"
				breadcrumbs={
					<Link
						to="/community/discussions"
						className="inline-flex items-center gap-1 hover:underline"
					>
						<ArrowLeft className="size-3.5" /> 返回讨论区
					</Link>
				}
				actions={
					<Toolbar>
						<Button onClick={() => setFormOpen(true)}>
							<Pencil /> 新建讨论
						</Button>
					</Toolbar>
				}
			/>

			<SectionCard
				title="我发布的讨论"
				description="列表由后端按当前账号过滤（author_id）。"
				actions={
					<Toolbar>
						<RefreshingBadge active={query.isFetching && !query.isPending} />
						<SearchField
							value={search}
							placeholder="搜索我的标题…"
							onChange={(value) => {
								setSearch(value);
								setPage(1);
							}}
						/>
					</Toolbar>
				}
				contentClassName="px-0"
				footer={
					<PaginationBar
						page={page}
						pageSize={pageSize}
						total={query.data?.meta.total ?? query.data?.items.length ?? 0}
						onPageChange={setPage}
						onPageSizeChange={(size) => {
							setPageSize(size);
							setPage(1);
						}}
					/>
				}
			>
				{meId === undefined ? (
					authQuery.isError ? (
						<ErrorBlock
							error={authQuery.error}
							title="读取当前账号失败"
							onRetry={() => void authQuery.refetch()}
						/>
					) : (
						<LoadingBlock label="读取当前账号…" />
					)
				) : (
					<QueryState
						query={query}
						skeleton={<TableSkeleton rows={5} columns={5} />}
						errorTitle="加载我的讨论失败"
						isEmpty={(result) => result.items.length === 0}
						empty={
							trimmed === "" ? (
								<EmptyBlock
									title="你还没有发过讨论"
									description="把你的解题思路或遇到的问题写下来，其他选手可以回复你。"
									icon={<MessagesSquare className="size-5" />}
									action={
										<Button size="sm" onClick={() => setFormOpen(true)}>
											新建讨论
										</Button>
									}
								/>
							) : (
								<EmptyBlock
									variant="filtered"
									description={`你的讨论里没有标题包含「${trimmed}」的条目。`}
									action={
										<Button size="sm" variant="outline" onClick={() => setSearch("")}>
											清除搜索
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
								onRowClick={(row) => void navigate(`/community/discussions/${row.id}`)}
								mobileCard={(row) => discussionMobileCard(row, meId)}
								rowActions={(row) => (
									<div className="flex items-center gap-1">
										<Button variant="ghost" size="sm" asChild>
											<Link to={`/community/discussions/${row.id}`}>查看</Link>
										</Button>
										<Button
											variant="ghost"
											size="sm"
											onClick={() =>
												setEditing({ id: row.id, title: row.title, content: row.content })
											}
										>
											<Pencil /> 编辑
										</Button>
										<Button
											variant="ghost"
											size="icon-sm"
											aria-label="删除讨论"
											title="删除讨论"
											disabled={deleteAction.isPending}
											onClick={() => void deleteAction.requestDelete(row)}
										>
											<Trash2 className="text-destructive" />
										</Button>
									</div>
								)}
							/>
						)}
					</QueryState>
				)}
			</SectionCard>

			<DiscussionFormSheet open={formOpen} onOpenChange={setFormOpen} />
			<DiscussionFormSheet
				open={editing !== null}
				onOpenChange={(open) => {
					if (!open) setEditing(null);
				}}
				discussion={editing ?? undefined}
			/>
		</PageBody>
	);
}

/** 讨论详情：左侧正文（Markdown），右侧评论流（`lg:grid-cols-[2fr_1fr]`）。 */
export function DiscussionDetailPage(): ReactNode {
	const params = useParams<{ id: string }>();
	const discussionId = params.id ?? "";
	const client = useClient();
	const me = useMe();
	const navigate = useNavigate();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const [editOpen, setEditOpen] = useState(false);
	const [newComment, setNewComment] = useState("");
	const [submittingComment, setSubmittingComment] = useState(false);

	const discussionQuery = useQuery({
		queryKey: qk.discussions.detail(discussionId),
		queryFn: () =>
			call<DiscussionWithAuthor>(client.service.discussions.get(discussionId), "讨论详情"),
		enabled: discussionId !== "",
	});

	const commentsQuery = useQuery({
		queryKey: qk.discussions.comments(discussionId),
		// SDK 声明 `DiscussionComments[]`，后端实际返回 `CommentWithAuthor`（含作者昵称 / 头像）：
		// 用 `call<T>()` 的 `Promise<unknown>` 重载取真实 DTO（见报告 PUBLIC SDK GAP）。
		queryFn: () =>
			call<DiscussionComment[]>(
				client.service.discussions.getComments(discussionId),
				"评论列表",
			),
		enabled: discussionId !== "",
	});

	const discussion = discussionQuery.data;
	useDocumentTitle(discussion ? `${discussion.title} · FloatCTF` : "讨论详情 · FloatCTF");

	const like = useMutation({
		mutationFn: () => callVoid(client.service.discussions.like(discussionId), "点赞"),
		onSuccess: () => {
			toast.success("已点赞");
			void queryClient.invalidateQueries({ queryKey: qk.discussions.detail(discussionId) });
		},
		onError: (error) => toast.apiError("点赞失败", error),
	});

	const unlike = useMutation({
		mutationFn: () => callVoid(client.service.discussions.unlike(discussionId), "取消点赞"),
		onSuccess: () => {
			toast.success("已取消点赞");
			void queryClient.invalidateQueries({ queryKey: qk.discussions.detail(discussionId) });
		},
		onError: (error) => toast.apiError("取消点赞失败", error),
	});

	const createComment = useMutation({
		mutationFn: (input: { content: string; parent_id?: string }) =>
			call(client.service.discussions.createComment(discussionId, input), "发表评论"),
		onSuccess: (_data, input) => {
			toast.success(input.parent_id ? "回复已发表" : "评论已发表");
			if (!input.parent_id) setNewComment("");
			void queryClient.invalidateQueries({ queryKey: qk.discussions.comments(discussionId) });
			void queryClient.invalidateQueries({ queryKey: qk.discussions.detail(discussionId) });
		},
		onError: (error) => toast.apiError("发表评论失败", error),
	});

	const patchComment = useMutation({
		mutationFn: (input: { commentId: string; content: string }) =>
			call(
				client.service.discussions.patchComment(discussionId, input.commentId, {
					content: input.content,
				}),
				"更新评论",
			),
		onSuccess: () => {
			toast.success("评论已更新");
			void queryClient.invalidateQueries({ queryKey: qk.discussions.comments(discussionId) });
		},
		onError: (error) => toast.apiError("更新评论失败", error),
	});

	const deleteComment = useMutation({
		mutationFn: (commentId: string) =>
			callVoid(client.service.discussions.deleteComment(discussionId, commentId), "删除评论"),
		onSuccess: () => {
			toast.success("评论已删除");
			void queryClient.invalidateQueries({ queryKey: qk.discussions.comments(discussionId) });
			void queryClient.invalidateQueries({ queryKey: qk.discussions.detail(discussionId) });
		},
		onError: (error) => toast.apiError("删除评论失败", error),
	});

	const removeDiscussion = useMutation({
		mutationFn: () => callVoid(client.service.discussions.remove(discussionId), "删除讨论"),
		onSuccess: () => {
			toast.success("讨论已删除");
			void queryClient.invalidateQueries({ queryKey: qk.discussions.all });
			void navigate("/community/discussions/mine", { replace: true });
		},
		onError: (error) => toast.apiError("删除讨论失败", error),
	});

	const isAuthor = me !== null && discussion !== undefined && discussion.author_id === me.id;

	async function handleCreateComment() {
		if (newComment.trim() === "") {
			toast.warning("评论内容不能为空");
			return;
		}
		setSubmittingComment(true);
		try {
			await createComment.mutateAsync({ content: newComment.trim() });
		} catch {
			// 失败原因已由 onError 的 toast.apiError 展示；这里保留输入内容不清空。
		} finally {
			setSubmittingComment(false);
		}
	}

	async function handleDeleteComment(commentId: string) {
		const ok = await confirm({
			title: "删除这条评论？",
			description: "删除后无法恢复。",
			consequences: [
				"该评论会被永久删除",
				"它下面的全部回复会被级联删除（父评论外键 ON DELETE CASCADE）",
			],
			tone: "danger",
			confirmText: "删除评论",
		});
		if (ok) deleteComment.mutate(commentId);
	}

	return (
		<PageBody>
			<PageHeader
				title={discussion?.title ?? "讨论详情"}
				description={
					discussion ? (
						<span className="inline-flex flex-wrap items-center gap-x-3 gap-y-1">
							<span className="inline-flex items-center gap-1.5">
								<UserAvatar
									name={discussion.author_nickname}
									avatar={discussion.author_avatar ?? null}
									size="sm"
								/>
								<span className="text-foreground">{discussion.author_nickname}</span>
							</span>
							<span className="text-xs text-muted-foreground">
								发布于 <RelativeTime value={discussion.created_at} />
							</span>
							<span className="text-xs text-muted-foreground">
								更新于 <RelativeTime value={discussion.updated_at} />
							</span>
						</span>
					) : (
						"帖子正文与评论。"
					)
				}
				breadcrumbs={
					<Link
						to="/community/discussions"
						className="inline-flex items-center gap-1 hover:underline"
					>
						<ArrowLeft className="size-3.5" /> 返回讨论区
					</Link>
				}
				actions={
					discussion ? (
						<Toolbar>
							<Button
								variant={discussion.is_liked ? "default" : "outline"}
								disabled={like.isPending || unlike.isPending}
								onClick={() => {
									if (discussion.is_liked) unlike.mutate();
									else like.mutate();
								}}
							>
								<ThumbsUp /> {discussion.is_liked ? "已点赞" : "点赞"} {discussion.like_count}
							</Button>
							{isAuthor ? (
								<Button variant="outline" onClick={() => setEditOpen(true)}>
									<Pencil /> 编辑
								</Button>
							) : null}
							{isAuthor ? (
								<Button
									variant="destructive"
									disabled={removeDiscussion.isPending}
									onClick={async () => {
										const ok = await confirm({
											title: "删除该讨论？",
											description: discussion.title,
											consequences: [
												"该讨论及其全部评论会被删除，不可恢复",
												"其他选手将无法再查看这个帖子",
											],
											tone: "danger",
											confirmText: "删除讨论",
										});
										if (ok) removeDiscussion.mutate();
									}}
								>
									<Trash2 /> 删除
								</Button>
							) : null}
						</Toolbar>
					) : null
				}
			/>

			<QueryState
				query={discussionQuery}
				skeleton={<LoadingBlock label="加载讨论…" />}
				errorTitle="加载讨论失败"
			>
				{(data) => (
					<div className="grid gap-5 lg:grid-cols-[2fr_1fr]">
						<SectionCard
							title="正文"
							description={`浏览 ${data.view_count} 次 · 点赞 ${data.like_count} · 评论 ${data.comment_count}`}
							contentClassName="space-y-3"
						>
							<div className="rounded-md border bg-muted/20 px-4 py-3">
								<MarkdownView>{data.content}</MarkdownView>
							</div>
							{data.updated_at !== data.created_at ? (
								<p className="text-xs text-muted-foreground">
									最后更新于 <RelativeTime value={data.updated_at} />
								</p>
							) : null}
						</SectionCard>

						<SectionCard
							title={`评论（${commentsQuery.data?.length ?? data.comment_count}）`}
							description="发表你的看法，或回复其他选手。"
							actions={
								<RefreshingBadge
									active={commentsQuery.isFetching && !commentsQuery.isPending}
								/>
							}
						>
							<div className="space-y-4">
								<CommentComposer
									value={newComment}
									onChange={setNewComment}
									onSubmit={() => void handleCreateComment()}
									isPending={submittingComment || createComment.isPending}
								/>

								<QueryState
									query={commentsQuery}
									skeleton={<LoadingBlock label="加载评论…" />}
									errorTitle="加载评论失败"
									isEmpty={(list) => list.length === 0}
									empty={
										<EmptyBlock
											title="还没有评论"
											description="第一条评论往往能带动整场讨论。"
										/>
									}
								>
									{(list) => (
										<div className="space-y-3">
											{buildCommentTree(list).map((node) => (
												<CommentCard
													key={node.id}
													comment={node}
													depth={0}
													currentUserId={me?.id}
													onReply={(parentId, content) =>
														createComment
															.mutateAsync({ content, parent_id: parentId })
															.then(() => undefined)
													}
													onUpdate={(commentId, content) =>
														patchComment
															.mutateAsync({ commentId, content })
															.then(() => undefined)
													}
													onDelete={(commentId) => void handleDeleteComment(commentId)}
												/>
											))}
										</div>
									)}
								</QueryState>
							</div>
						</SectionCard>
					</div>
				)}
			</QueryState>

			{discussion ? (
				<DiscussionFormSheet
					open={editOpen}
					onOpenChange={setEditOpen}
					discussion={{
						id: discussion.id,
						title: discussion.title,
						content: discussion.content,
					}}
				/>
			) : null}
		</PageBody>
	);
}
