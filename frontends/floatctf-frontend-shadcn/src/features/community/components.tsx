/**
 * 社区域复用片段 —— 搜索框、讨论表单 Sheet、评论树与评论卡片。
 *
 * 这一层只负责**交互编排与展示**：数据全部由页面传入（真实接口在页面层调用），
 * 因此这里不存在任何本地假数据，也不缓存业务状态。
 *
 * 评论 DTO 说明（见交付报告的 `PUBLIC SDK GAP`）：后端
 * `GET /discussions/{id}/comments` 实际返回 `CommentWithAuthor`
 * （`discussion_comments` flatten + `author_nickname` / `author_avatar`），
 * 但 SDK 把返回类型声明成 `DiscussionComments[]`，缺少作者字段。
 * 页面用 `call<T>()` 的 `Promise<unknown>` 重载取真实 DTO，这里只声明类型。
 */

import { useEffect, useState, type ReactNode } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CornerDownRight, Pencil, Search, Trash2 } from "lucide-react";

import type { DiscussionComments, Discussions } from "@floatctf/sdk/entity";

import { call } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill } from "~/components/app/badges";
import { Field, FormFooter, FormSheet } from "~/components/app/form";
import { MarkdownEditor, MarkdownView } from "~/components/app/markdown";
import { InlineError } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { RelativeTime, UserAvatar } from "~/components/app/user-cell";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import { cn } from "~/lib/utils";

/** 受控搜索框（配合 `useDebouncedValue` 使用，不自己防抖）。 */
export function SearchField({
	value,
	onChange,
	placeholder = "搜索…",
	className,
}: {
	value: string;
	onChange: (value: string) => void;
	placeholder?: string;
	className?: string;
}) {
	return (
		<div className={cn("relative w-full sm:w-64", className)}>
			<Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
			<Input
				value={value}
				placeholder={placeholder}
				aria-label={placeholder}
				className="pl-8"
				onChange={(event) => onChange(event.target.value)}
			/>
		</div>
	);
}

export interface DiscussionFormInput {
	title: string;
	content: string;
}

/**
 * 新建 / 编辑讨论（Sheet 表单：标题 + Markdown 正文）。
 * 传入 `discussion` 即进入编辑模式（`patch` 必须带 `id`）。
 */
export function DiscussionFormSheet({
	open,
	onOpenChange,
	discussion,
	onSaved,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	discussion?: { id: string; title: string; content: string };
	onSaved?: (discussionId: string, mode: "create" | "edit") => void;
}): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const editing = discussion !== undefined;
	const [title, setTitle] = useState("");
	const [content, setContent] = useState("");
	const [validationError, setValidationError] = useState<string | null>(null);

	// 每次打开都回到当前条目的初始值（同一个 Sheet 复用「新建 / 编辑」两种场景）。
	useEffect(() => {
		if (!open) return;
		setTitle(discussion?.title ?? "");
		setContent(discussion?.content ?? "");
		setValidationError(null);
	}, [open, discussion?.id, discussion?.title, discussion?.content]);

	const mutation = useMutation({
		mutationFn: (input: { id?: string; title: string; content: string }): Promise<Discussions> =>
			input.id
				? call<Discussions>(
						client.service.discussions.patch({
							id: input.id,
							title: input.title,
							content: input.content,
						}),
						"更新讨论",
					)
				: call<Discussions>(
						client.service.discussions.create({
							title: input.title,
							content: input.content,
						}),
						"发布讨论",
					),
		onSuccess: (saved, input) => {
			toast.success(input.id ? "讨论已更新" : "讨论已发布");
			// `qk.discussions.all` 是列表 / 我的 / 详情 / 评论键的前缀，一次失效即可覆盖。
			void queryClient.invalidateQueries({ queryKey: qk.discussions.all });
			onOpenChange(false);
			onSaved?.(saved.id, input.id ? "edit" : "create");
		},
		onError: (error) => toast.apiError(editing ? "更新讨论失败" : "发布讨论失败", error),
	});

	return (
		<FormSheet
			open={open}
			onOpenChange={onOpenChange}
			width="xl"
			title={editing ? "编辑讨论" : "新建讨论"}
			description="标题与正文都会公开给其他选手；正文支持 Markdown（GFM）。"
			footer={
				<FormFooter
					formId="discussion-form"
					submitLabel={editing ? "保存修改" : "发布讨论"}
					isPending={mutation.isPending}
					onCancel={() => onOpenChange(false)}
					hint={editing ? "仅作者本人可以修改自己的讨论。" : "发布后可随时编辑或删除。"}
				/>
			}
		>
			<form
				id="discussion-form"
				className="space-y-4"
				onSubmit={(event) => {
					event.preventDefault();
					const nextTitle = title.trim();
					const nextContent = content.trim();
					if (nextTitle === "" || nextContent === "") {
						setValidationError("标题与正文都不能为空。");
						return;
					}
					setValidationError(null);
					mutation.mutate({
						...(editing && discussion ? { id: discussion.id } : {}),
						title: nextTitle,
						content: nextContent,
					});
				}}
			>
				<Field
					label="标题"
					htmlFor="discussion-title"
					required
					hint="一句话说清主题，便于他人检索。"
					error={validationError && title.trim() === "" ? validationError : undefined}
				>
					<Input
						id="discussion-title"
						value={title}
						maxLength={200}
						disabled={mutation.isPending}
						placeholder="例如：如何稳定复现某道题的越界？"
						onChange={(event) => setTitle(event.target.value)}
					/>
				</Field>
				<Field
					label="正文（Markdown）"
					required
					hint="支持 Markdown / GFM；插图会上传到平台图床。"
					error={validationError && content.trim() === "" ? validationError : undefined}
				>
					<MarkdownEditor
						value={content}
						onChange={setContent}
						minHeight={320}
						disabled={mutation.isPending}
						placeholder="描述背景、复现步骤与你的结论…"
					/>
				</Field>
			</form>
		</FormSheet>
	);
}

/** 评论真实 DTO：`discussion_comments` + 后端附带的作者信息。 */
export type DiscussionComment = DiscussionComments & {
	author_nickname?: string;
	author_avatar?: string | null;
};

export interface CommentNode extends DiscussionComment {
	replies: CommentNode[];
}

/**
 * 按 `parent_id` 组装评论树。
 * 后端删除父评论时会级联删除其回复（`parent_id` ON DELETE CASCADE），
 * 所以「父评论缺失」只是防御性兜底：这种节点按顶层展示，不会丢评论。
 */
export function buildCommentTree(comments: DiscussionComment[]): CommentNode[] {
	const nodes = new Map<string, CommentNode>();
	for (const comment of comments) nodes.set(comment.id, { ...comment, replies: [] });

	const roots: CommentNode[] = [];
	for (const comment of comments) {
		const node = nodes.get(comment.id);
		if (!node) continue;
		const parent = comment.parent_id ? nodes.get(comment.parent_id) : undefined;
		if (parent) parent.replies.push(node);
		else roots.push(node);
	}

	const byCreatedAt = (a: CommentNode, b: CommentNode) =>
		Date.parse(a.created_at) - Date.parse(b.created_at);
	roots.sort(byCreatedAt);
	for (const node of nodes.values()) node.replies.sort(byCreatedAt);
	return roots;
}

/** 顶层评论输入区。 */
export function CommentComposer({
	value,
	onChange,
	onSubmit,
	isPending,
	placeholder = "写下你的看法（支持 Markdown）…",
}: {
	value: string;
	onChange: (value: string) => void;
	onSubmit: () => void;
	isPending: boolean;
	placeholder?: string;
}): ReactNode {
	return (
		<div className="space-y-2">
			<MarkdownEditor
				value={value}
				onChange={onChange}
				minHeight={140}
				allowUpload={false}
				disabled={isPending}
				placeholder={placeholder}
			/>
			<div className="flex justify-end">
				<Button type="button" size="sm" disabled={isPending} onClick={onSubmit}>
					{isPending ? "提交中…" : "发表评论"}
				</Button>
			</div>
		</div>
	);
}

/** 单条评论（递归渲染回复）。 */
export function CommentCard({
	comment,
	depth,
	currentUserId,
	onReply,
	onUpdate,
	onDelete,
}: {
	comment: CommentNode;
	depth: number;
	currentUserId: string | undefined;
	onReply: (parentId: string, content: string) => Promise<void>;
	onUpdate: (commentId: string, content: string) => Promise<void>;
	onDelete: (commentId: string) => void;
}): ReactNode {
	const [replyOpen, setReplyOpen] = useState(false);
	const [replyText, setReplyText] = useState("");
	const [editing, setEditing] = useState(false);
	const [editText, setEditText] = useState(comment.content);
	const [localError, setLocalError] = useState<unknown>(null);
	const [busy, setBusy] = useState(false);

	const authorName = comment.author_nickname ?? comment.author_id;
	const mine = currentUserId !== undefined && currentUserId === comment.author_id;
	const edited = comment.updated_at !== comment.created_at;
	const nestedClass = depth > 0 && depth < 4 ? "ml-3 border-l pl-3" : "";

	async function submitReply() {
		if (replyText.trim() === "") {
			toast.warning("回复内容不能为空");
			return;
		}
		setBusy(true);
		setLocalError(null);
		try {
			await onReply(comment.id, replyText.trim());
			setReplyText("");
			setReplyOpen(false);
		} catch (error) {
			// 后端拒绝（403 / 400）时保留输入并就地显示原因；toast 由页面层统一提示。
			setLocalError(error);
		} finally {
			setBusy(false);
		}
	}

	async function submitEdit() {
		if (editText.trim() === "") {
			toast.warning("评论内容不能为空");
			return;
		}
		setBusy(true);
		setLocalError(null);
		try {
			await onUpdate(comment.id, editText.trim());
			setEditing(false);
		} catch (error) {
			setLocalError(error);
		} finally {
			setBusy(false);
		}
	}

	return (
		<div className="space-y-2">
			<div className={cn("rounded-lg border p-3", mine && "border-primary/30")}>
				<div className="flex items-start gap-2">
					<UserAvatar
						name={authorName}
						avatar={comment.author_avatar ?? null}
						size="sm"
						className="mt-0.5"
					/>
					<div className="min-w-0 flex-1 space-y-1.5">
						<div className="flex flex-wrap items-center gap-2">
							<span className="truncate text-sm font-medium">{authorName}</span>
							<RelativeTime value={comment.created_at} />
							{edited ? <TonePill tone="muted">已编辑</TonePill> : null}
							{mine ? <TonePill tone="info">我</TonePill> : null}
						</div>

						{editing ? (
							<div className="space-y-2">
								<MarkdownEditor
									value={editText}
									onChange={setEditText}
									minHeight={140}
									allowUpload={false}
									disabled={busy}
								/>
								<div className="flex items-center gap-2">
									<Button
										type="button"
										size="sm"
										disabled={busy}
										onClick={() => void submitEdit()}
									>
										{busy ? "保存中…" : "保存"}
									</Button>
									<Button
										type="button"
										size="sm"
										variant="outline"
										disabled={busy}
										onClick={() => {
											setEditText(comment.content);
											setEditing(false);
											setLocalError(null);
										}}
									>
										取消
									</Button>
								</div>
							</div>
						) : (
							<MarkdownView className="text-sm">{comment.content}</MarkdownView>
						)}
					</div>
				</div>

				{!editing ? (
					<div className="mt-2 flex flex-wrap items-center gap-1 pl-6">
						<Button
							type="button"
							variant="ghost"
							size="xs"
							onClick={() => {
								setReplyOpen((prev) => !prev);
								setLocalError(null);
							}}
						>
							<CornerDownRight /> 回复
						</Button>
						{mine ? (
							<Button
								type="button"
								variant="ghost"
								size="xs"
								onClick={() => {
									setEditText(comment.content);
									setEditing(true);
									setLocalError(null);
								}}
							>
								<Pencil /> 编辑
							</Button>
						) : null}
						{mine ? (
							<Button
								type="button"
								variant="ghost"
								size="xs"
								className="text-destructive hover:text-destructive"
								aria-label="删除评论"
								onClick={() => onDelete(comment.id)}
							>
								<Trash2 /> 删除
							</Button>
						) : null}
						{comment.replies.length > 0 ? (
							<span className="text-xs text-muted-foreground">
								{comment.replies.length} 条回复
							</span>
						) : null}
					</div>
				) : null}

				{localError ? <InlineError error={localError} className="mt-2 pl-6" /> : null}

				{replyOpen ? (
					<div className="mt-3 space-y-2 pl-6">
						<MarkdownEditor
							value={replyText}
							onChange={setReplyText}
							minHeight={120}
							allowUpload={false}
							disabled={busy}
							placeholder={`回复 ${authorName}…`}
						/>
						<div className="flex items-center gap-2">
							<Button type="button" size="sm" disabled={busy} onClick={() => void submitReply()}>
								{busy ? "提交中…" : "回复"}
							</Button>
							<Button
								type="button"
								size="sm"
								variant="outline"
								disabled={busy}
								onClick={() => {
									setReplyOpen(false);
									setReplyText("");
									setLocalError(null);
								}}
							>
								取消
							</Button>
						</div>
					</div>
				) : null}
			</div>

			{comment.replies.length > 0 ? (
				<div className={cn("space-y-2", nestedClass)}>
					{comment.replies.map((reply) => (
						<CommentCard
							key={reply.id}
							comment={reply}
							depth={depth + 1}
							currentUserId={currentUserId}
							onReply={onReply}
							onUpdate={onUpdate}
							onDelete={onDelete}
						/>
					))}
				</div>
			) : null}
		</div>
	);
}
