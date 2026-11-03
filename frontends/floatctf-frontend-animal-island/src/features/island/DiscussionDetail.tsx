/**
 * 讨论详情 + 评论。
 *
 * 一个诚实的处理：`DiscussionComments` 实体只有 `author_id`，**没有作者昵称**。
 * 因此这里在运行时**试探**可能的昵称字段（后端可能比生成实体多返回），
 * 拿不到就退化成短 id 并加 tooltip 说明 —— 而不是显示一个假名字或留空。
 * 这是"生成实体与接口返回不一致"，不是前端能修的，已在此标明。
 */
import { useState } from "react";
import { useNavigate, useParams } from "react-router";
import {
	Avatar,
	Button,
	Card,
	Divider,
	Input,
	Loading,
	Notification,
	Tag,
	Tooltip,
} from "animal-island-ui";
import {
	ArrowLeftIcon,
	ChatIcon,
	EditIcon,
	EyeIcon,
	HeartIcon,
	RefreshIcon,
	TrashIcon,
} from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { EmptyState, ErrorState, SkeletonCards } from "~/components/states";
import { useSession } from "~/auth/store";
import { formatTime } from "~/lib/event";
// `DiscussionComments` 是生成实体，只从 `/entity` 子路径导出（不在根导出面里）。
import type { DiscussionComments } from "@floatctf/sdk/entity";

/** 评论作者展示名：优先取运行时可能存在的昵称字段，否则退化为短 id。 */
function commentAuthor(c: DiscussionComments): { name: string; degraded: boolean } {
	const maybe = c as unknown as { author_nickname?: string; nickname?: string };
	const name = maybe.author_nickname ?? maybe.nickname;
	if (name) return { name, degraded: false };
	return { name: c.author_id.slice(0, 8), degraded: true };
}

export function DiscussionDetail() {
	const { discussionId = "" } = useParams();
	const runtime = useRuntime();
	const navigate = useNavigate();
	const confirm = useConfirm();
	const meId = useSession((s) => s.me?.id);

	const [draft, setDraft] = useState("");
	const [editing, setEditing] = useState<string | null>(null);
	const [editText, setEditText] = useState("");

	const discussion = useUniQuery(
		["island", "discussion", discussionId],
		() => runtime.client.service.discussions.get(discussionId),
		"加载讨论",
	);
	const comments = useUniQuery(
		["island", "discussion", discussionId, "comments"],
		() => runtime.client.service.discussions.getComments(discussionId),
		"加载评论",
	);

	const like = useUniMutation(
		() =>
			discussion.data?.is_liked
				? runtime.client.service.discussions.unlike(discussionId)
				: runtime.client.service.discussions.like(discussionId),
		"点赞",
	);
	const createComment = useUniMutation(
		(vars: { id: string; content: string }) =>
			runtime.client.service.discussions.createComment(vars.id, { content: vars.content }),
		"发表评论",
	);
	const patchComment = useUniMutation(
		(vars: { commentId: string; content: string }) =>
			runtime.client.service.discussions.patchComment(discussionId, vars.commentId, {
				content: vars.content,
			}),
		"修改评论",
	);
	const removeComment = useUniMutation(
		(vars: { commentId: string }) =>
			runtime.client.service.discussions.deleteComment(discussionId, vars.commentId),
		"删除评论",
	);
	const removeDiscussion = useUniMutation(
		() => runtime.client.service.discussions.remove(discussionId),
		"删除讨论",
	);

	if (discussion.isLoading) return <Loading active tip="加载讨论…" />;
	if (discussion.error) {
		return <ErrorState error={discussion.error} onRetry={() => void discussion.refetch()} />;
	}
	if (!discussion.data) {
		return (
			<Card>
				<EmptyState
					title="讨论不存在"
					description="它可能已被作者或管理员删除。"
					action={<Button onClick={() => void navigate("/island/discussions")}>回讨论区</Button>}
				/>
			</Card>
		);
	}

	const d = discussion.data;
	const mine = meId === d.author_id;

	return (
		<>
			<PageHeader
				title={d.title}
				description={`由 ${d.author_nickname} 发布于 ${formatTime(d.created_at)}`}
				extra={
					<>
						<Button
							size="small"
							icon={<ArrowLeftIcon size={16} />}
							onClick={() => void navigate("/island/discussions")}
						>
							回列表
						</Button>
						<Button
							size="small"
							icon={<RefreshIcon size={16} />}
							onClick={() => {
								void discussion.refetch();
								void comments.refetch();
							}}
						>
							刷新
						</Button>
						{mine ? (
							<Button
								size="small"
								danger
								icon={<TrashIcon size={16} />}
								onClick={async () => {
									const ok = await confirm({
										title: "删除这条讨论",
										consequence:
											"讨论正文与**它下面的全部评论**会一起消失，无法恢复。",
										okText: "删除讨论",
										danger: true,
										confirmPhrase: "删除",
									});
									if (!ok) return;
									await removeDiscussion.mutateAsync(undefined);
									Notification.success({ message: "已删除" });
									void navigate("/island/discussions");
								}}
							>
								删除
							</Button>
						) : null}
					</>
				}
			/>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<div className="ai-row" style={{ gap: 12 }}>
						<Avatar src={d.author_avatar ?? undefined}>
							{d.author_nickname?.[0] ?? "?"}
						</Avatar>
						<div className="ai-col" style={{ gap: 2 }}>
							<span className="ai-strong">{d.author_nickname}</span>
							<Tag size="small" variant="outlined">
								<EyeIcon size={12} /> {d.view_count}
							</Tag>
						</div>
					</div>
					<Divider />
					<pre style={{ margin: 0, whiteSpace: "pre-wrap", fontFamily: "inherit" }}>
						{d.content}
					</pre>
					<Divider />
					<div className="ai-row">
						<Button
							type={d.is_liked ? "primary" : "default"}
							icon={<HeartIcon size={16} />}
							loading={like.isPending}
							onClick={async () => {
								await like.mutateAsync(undefined);
								void discussion.refetch();
							}}
						>
							{d.is_liked ? "已赞" : "点赞"} {d.like_count}
						</Button>
						<Tag size="small" variant="outlined">
							<ChatIcon size={12} /> {d.comment_count} 条评论
						</Tag>
					</div>
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<span className="ai-strong">评论</span>

					<div className="ai-row" style={{ gap: 8 }}>
						<div style={{ flex: 1, minWidth: 240 }}>
							<Input
								allowClear
								placeholder="写下你的看法…"
								value={draft}
								onChange={(e) => setDraft(e.target.value)}
							/>
						</div>
						<Button
							type="primary"
							disabled={!draft.trim()}
							loading={createComment.isPending}
							onClick={async () => {
								await createComment.mutateAsync({ id: discussionId, content: draft.trim() });
								setDraft("");
								void comments.refetch();
								void discussion.refetch();
							}}
						>
							发表
						</Button>
					</div>

					<Divider />

					{comments.isLoading ? (
						<SkeletonCards count={2} />
					) : comments.error ? (
						<ErrorState error={comments.error} onRetry={() => void comments.refetch()} />
					) : (comments.data?.length ?? 0) === 0 ? (
						<EmptyState title="还没有评论" description="来做第一个回复的人。" />
					) : (
						(comments.data ?? []).map((c) => {
							const author = commentAuthor(c);
							const isMine = meId === c.author_id;
							return (
								<div key={c.id} className="ai-col" style={{ gap: 6 }}>
									<div className="ai-row-between">
										<div className="ai-row" style={{ gap: 8 }}>
											<Avatar size="small">{author.name[0] ?? "?"}</Avatar>
											{author.degraded ? (
												<Tooltip title="后端未返回作者昵称，这里显示的是账号 id 前缀">
													<span className="ai-muted">{author.name}</span>
												</Tooltip>
											) : (
												<span className="ai-strong">{author.name}</span>
											)}
											<span className="ai-muted" style={{ fontSize: 12 }}>
												{formatTime(c.created_at)}
											</span>
										</div>
										{isMine ? (
											<div className="ai-row" style={{ gap: 4 }}>
												<Button
													size="small"
													type="text"
													icon={<EditIcon size={14} />}
													onClick={() => {
														setEditing(c.id);
														setEditText(c.content);
													}}
												>
													编辑
												</Button>
												<Button
													size="small"
													type="text"
													danger
													icon={<TrashIcon size={14} />}
													onClick={async () => {
														const ok = await confirm({
															title: "删除这条评论",
															consequence: "评论会被永久删除，无法恢复。",
															okText: "删除",
															danger: true,
														});
														if (!ok) return;
														await removeComment.mutateAsync({ commentId: c.id });
														void comments.refetch();
														void discussion.refetch();
													}}
												>
													删除
												</Button>
											</div>
										) : null}
									</div>

									{editing === c.id ? (
										<div className="ai-row" style={{ gap: 8 }}>
											<div style={{ flex: 1, minWidth: 200 }}>
												<Input
													value={editText}
													onChange={(e) => setEditText(e.target.value)}
												/>
											</div>
											<Button
												size="small"
												type="primary"
												loading={patchComment.isPending}
												onClick={async () => {
													await patchComment.mutateAsync({
														commentId: c.id,
														content: editText.trim(),
													});
													setEditing(null);
													void comments.refetch();
												}}
											>
												保存
											</Button>
											<Button size="small" onClick={() => setEditing(null)}>
												取消
											</Button>
										</div>
									) : (
										<pre
											style={{
												margin: 0,
												whiteSpace: "pre-wrap",
												fontFamily: "inherit",
												paddingLeft: 40,
											}}
										>
											{c.content}
										</pre>
									)}
									<Divider />
								</div>
							);
						})
					)}
				</div>
			</Card>
		</>
	);
}
