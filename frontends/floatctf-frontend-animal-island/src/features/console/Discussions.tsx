/**
 * 讨论治理。
 *
 * 管理员在这里只做**两件事**：删讨论、删评论。刻意不做"编辑他人内容" ——
 * 平台没有这个能力，前端也不该假装有（真正的判定与能力都在服务端）。
 *
 * 删除一律二次确认并写清后果（讨论会连带它的全部评论一起消失）。
 */
import { useState } from "react";
import { Button, Card, Divider, Drawer, Input, Loading, Modal, Tag } from "animal-island-ui";
import { ChatIcon, EyeIcon, HeartIcon, RefreshIcon, SearchIcon, TrashIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { Can } from "~/app/permissions";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { EmptyState, ErrorState, SkeletonCards } from "~/components/states";
import { formatTime } from "~/lib/event";
import type { Discussions } from "@floatctf/sdk/entity";

/** 某条讨论的评论抽屉：可以单独删除评论。 */
function CommentsDrawer({
	discussionId,
	onClose,
	onChanged,
}: {
	discussionId: string | null;
	onClose: () => void;
	onChanged: () => void;
}) {
	const runtime = useRuntime();
	const confirm = useConfirm();
	const comments = useUniQuery(
		["console", "discussion", discussionId ?? "", "comments"],
		() => runtime.client.admin.discussions.getComments(discussionId ?? ""),
		"加载评论",
		{ enabled: Boolean(discussionId) },
	);
	const removeComment = useUniMutation(
		(vars: { discussionId: string; commentId: string }) =>
			runtime.client.admin.discussions.removeComment(vars.discussionId, vars.commentId),
		"删除评论",
	);

	return (
		<Drawer open={discussionId !== null} onClose={onClose} title="评论治理" width={520}>
			{comments.isLoading ? (
				<Loading active tip="加载评论…" />
			) : comments.error ? (
				<ErrorState error={comments.error} onRetry={() => void comments.refetch()} />
			) : (comments.data?.length ?? 0) === 0 ? (
				<EmptyState title="这条讨论没有评论" />
			) : (
				<div className="ai-col" style={{ gap: 10 }}>
					{(comments.data ?? []).map((c) => (
						<Card key={c.id}>
							<div className="ai-col" style={{ gap: 6 }}>
								<div className="ai-row-between">
									<span className="ai-muted ai-mono">
										{c.author_id.slice(0, 8)}
									</span>
									<span className="ai-muted" style={{ fontSize: 12 }}>
										{formatTime(c.created_at)}
									</span>
								</div>
								<pre style={{ margin: 0, whiteSpace: "pre-wrap", fontFamily: "inherit" }}>
									{c.content}
								</pre>
								<div className="ai-row" style={{ justifyContent: "flex-end" }}>
									<Can permission="discussions.moderate">
									<Button
										size="small"
										type="text"
										danger
										icon={<TrashIcon size={14} />}
										onClick={async () => {
											const ok = await confirm({
												title: "删除这条评论",
												consequence: "评论会被永久删除，讨论的评论计数也会同步更新。",
												okText: "删除",
												danger: true,
											});
											if (!ok) return;
											await removeComment.mutateAsync({
												discussionId: discussionId ?? "",
												commentId: c.id,
											});
											void comments.refetch();
											onChanged();
										}}
									>
										删除
									</Button>
									</Can>
								</div>
							</div>
						</Card>
					))}
				</div>
			)}
		</Drawer>
	);
}

export function ConsoleDiscussions() {
	const runtime = useRuntime();
	const confirm = useConfirm();
	const [keyword, setKeyword] = useState("");
	const [openOne, setOpenOne] = useState<Discussions | null>(null);
	const [commentsFor, setCommentsFor] = useState<string | null>(null);

	const query = useUniQuery(
		["console", "discussions"],
		() => runtime.client.admin.discussions.fetch(),
		"加载讨论",
	);
	const remove = useUniMutation(
		(vars: string[]) => runtime.client.admin.discussions.remove(vars),
		"删除讨论",
	);

	const rows = (query.data ?? []).filter((d) => {
		const k = keyword.trim().toLowerCase();
		if (!k) return true;
		return `${d.title}${d.content}`.toLowerCase().includes(k);
	});

	return (
		<>
			<PageHeader
				title="讨论治理"
				description="查看并删除违规内容。平台不提供「编辑他人内容」，这里也不会假装提供。"
				extra={
					<Button
						size="small"
						icon={<RefreshIcon size={16} />}
						onClick={() => void query.refetch()}
					>
						刷新
					</Button>
				}
			/>

			<Card>
				<Input
					allowClear
					prefix={<SearchIcon size={16} />}
					placeholder="搜索标题或正文"
					value={keyword}
					onChange={(e) => setKeyword(e.target.value)}
				/>
			</Card>

			{query.isLoading ? (
				<SkeletonCards count={4} />
			) : query.error ? (
				<ErrorState error={query.error} onRetry={() => void query.refetch()} />
			) : rows.length === 0 ? (
				<Card>
					<EmptyState
						title={keyword ? "没有匹配的讨论" : "还没有讨论"}
						description={keyword ? "换个关键词试试。" : "选手侧发布讨论后会出现在这里。"}
					/>
				</Card>
			) : (
				<div className="ai-col" style={{ gap: 12 }}>
					{rows.map((d) => (
						<Card key={d.id}>
							<div className="ai-col" style={{ gap: 8 }}>
								<div className="ai-row-between">
									<span className="ai-strong" style={{ fontSize: 16 }}>
										{d.title}
									</span>
									<span className="ai-muted" style={{ fontSize: 12 }}>
										{formatTime(d.created_at)}
									</span>
								</div>
								<span className="ai-muted" style={{ fontSize: 13 }}>
									{d.content.length > 160 ? `${d.content.slice(0, 160)}…` : d.content}
								</span>
								<Divider />
								<div className="ai-row-between">
									<div className="ai-row" style={{ gap: 8 }}>
										<Tag size="small" variant="outlined">
											<ChatIcon size={12} /> {d.comment_count}
										</Tag>
										<Tag size="small" variant="outlined">
											<EyeIcon size={12} /> {d.view_count}
										</Tag>
										<Tag size="small" variant="outlined">
											<HeartIcon size={12} /> {d.like_count}
										</Tag>
										<Tag size="small" variant="soft" color="brown">
											作者 {d.author_id.slice(0, 8)}
										</Tag>
									</div>
									<div className="ai-row" style={{ gap: 6 }}>
										<Button size="small" onClick={() => setOpenOne(d)}>
											查看正文
										</Button>
										<Button
											size="small"
											icon={<ChatIcon size={14} />}
											onClick={() => setCommentsFor(d.id)}
										>
											评论治理
										</Button>
										<Can permission="discussions.moderate">
										<Button
											size="small"
											danger
											icon={<TrashIcon size={14} />}
											onClick={async () => {
												const ok = await confirm({
													title: `删除讨论「${d.title}」`,
													consequence:
														"这条讨论与它下面的**全部评论**都会一起消失。作者不会收到通知，且无法恢复。",
													okText: "删除讨论",
													danger: true,
													confirmPhrase: "删除",
												});
												if (!ok) return;
												await remove.mutateAsync([d.id]);
												void query.refetch();
											}}
										>
											删除
										</Button>
										</Can>
									</div>
								</div>
							</div>
						</Card>
					))}
				</div>
			)}

			<Modal
				open={openOne !== null}
				title={openOne?.title ?? ""}
				typewriter={false}
				width={680}
				onClose={() => setOpenOne(null)}
				footer={<Button onClick={() => setOpenOne(null)}>关闭</Button>}
			>
				<div className="ai-col" style={{ gap: 10 }}>
					<span className="ai-muted" style={{ fontSize: 12 }}>
						作者 {openOne?.author_id} ｜ 发布于 {formatTime(openOne?.created_at ?? "")}
					</span>
					<Divider />
					<pre style={{ margin: 0, whiteSpace: "pre-wrap", fontFamily: "inherit" }}>
						{openOne?.content}
					</pre>
				</div>
			</Modal>

			<CommentsDrawer
				discussionId={commentsFor}
				onClose={() => setCommentsFor(null)}
				onChanged={() => void query.refetch()}
			/>
		</>
	);
}
