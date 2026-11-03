/**
 * 讨论区列表。
 *
 * 交互取舍：这里把"发帖"做成**行内展开**而不是跳新页面 —— 写一个讨论帖
 * 是一件小事，为它做一次路由跳转会让"随手问一句"变得很重。
 */
import { useMemo, useState } from "react";
import { useNavigate } from "react-router";
import {
	Avatar,
	Button,
	Card,
	Divider,
	Input,
	Modal,
	Notification,
	Tag,
} from "animal-island-ui";
import {
	ChatIcon,
	EyeIcon,
	HeartIcon,
	PlusIcon,
	SearchIcon,
} from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { PageHeader } from "~/app/shell";
import { EmptyState, ErrorState, SkeletonCards } from "~/components/states";
import { formatTime } from "~/lib/event";
import type { DiscussionWithAuthor } from "@floatctf/sdk";

function DiscussionCard({ item }: { item: DiscussionWithAuthor }) {
	const runtime = useRuntime();
	const navigate = useNavigate();
	const like = useUniMutation(
		() =>
			item.is_liked
				? runtime.client.service.discussions.unlike(item.id)
				: runtime.client.service.discussions.like(item.id),
		item.is_liked ? "取消点赞" : "点赞",
	);

	return (
		<Card hoverable>
			<div className="ai-col" style={{ gap: 8 }}>
				<div className="ai-row-between">
					<span className="ai-strong" style={{ fontSize: 16 }}>
						{item.title}
					</span>
					<span className="ai-muted" style={{ fontSize: 12 }}>
						{formatTime(item.created_at)}
					</span>
				</div>

				<span className="ai-muted" style={{ fontSize: 13 }}>
					{item.content.length > 120 ? `${item.content.slice(0, 120)}…` : item.content}
				</span>

				<Divider />

				<div className="ai-row-between">
					<div className="ai-row" style={{ gap: 8 }}>
						<Avatar size="small" src={item.author_avatar ?? undefined}>
							{item.author_nickname?.[0] ?? "?"}
						</Avatar>
						<span className="ai-muted">{item.author_nickname}</span>
					</div>
					<div className="ai-row" style={{ gap: 8 }}>
						<Tag size="small" variant="outlined">
							<ChatIcon size={12} /> {item.comment_count}
						</Tag>
						<Tag size="small" variant="outlined">
							<EyeIcon size={12} /> {item.view_count}
						</Tag>
						<Button
							size="small"
							type={item.is_liked ? "primary" : "default"}
							icon={<HeartIcon size={14} />}
							loading={like.isPending}
							onClick={() => void like.mutate(undefined)}
						>
							{item.like_count}
						</Button>
						<Button
							size="small"
							type="primary"
							onClick={() => void navigate(`/island/discussions/${item.id}`)}
						>
							查看
						</Button>
					</div>
				</div>
			</div>
		</Card>
	);
}

export function DiscussionList() {
	const runtime = useRuntime();
	const [keyword, setKeyword] = useState("");
	const [open, setOpen] = useState(false);
	const [title, setTitle] = useState("");
	const [content, setContent] = useState("");

	const query = useUniQuery(
		["island", "discussions"],
		() => runtime.client.service.discussions.fetch(),
		"加载讨论",
	);

	const create = useUniMutation(
		(vars: { title: string; content: string }) =>
			runtime.client.service.discussions.create(vars),
		"发布讨论",
	);

	const rows = useMemo(() => {
		const list = query.data ?? [];
		const k = keyword.trim().toLowerCase();
		if (!k) return list;
		return list.filter((d) => `${d.title}${d.content}`.toLowerCase().includes(k));
	}, [query.data, keyword]);

	return (
		<>
			<PageHeader
				title="讨论区"
				description="提问、分享与讨论。点赞与评论都会实时反映在列表上。"
				extra={
					<Button
						type="primary"
						icon={<PlusIcon size={16} />}
						onClick={() => setOpen(true)}
					>
						发起讨论
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
						title={keyword ? "没有匹配的讨论" : "还没有人发过讨论"}
						description={keyword ? "换个关键词试试。" : "来问第一个问题吧。"}
						action={
							<Button type="primary" icon={<PlusIcon size={16} />} onClick={() => setOpen(true)}>
								发起讨论
							</Button>
						}
					/>
				</Card>
			) : (
				<div className="ai-col" style={{ gap: 12 }}>
					{rows.map((d) => (
						<DiscussionCard key={d.id} item={d} />
					))}
				</div>
			)}

			<Modal
				open={open}
				title="发起讨论"
				typewriter={false}
				onClose={() => setOpen(false)}
				footer={
					<>
						<Button onClick={() => setOpen(false)}>取消</Button>
						<Button
							type="primary"
							disabled={!title.trim() || !content.trim()}
							loading={create.isPending}
							onClick={async () => {
								await create.mutateAsync({ title: title.trim(), content: content.trim() });
								setTitle("");
								setContent("");
								setOpen(false);
								Notification.success({ message: "已发布" });
								void query.refetch();
							}}
						>
							发布
						</Button>
					</>
				}
			>
				<div className="ai-col" style={{ gap: 10 }}>
					<label className="ai-col" style={{ gap: 6 }}>
						<span>标题</span>
						<Input
							value={title}
							placeholder="一句话说清你要问什么"
							onChange={(e) => setTitle(e.target.value)}
						/>
					</label>
					<label className="ai-col" style={{ gap: 6 }}>
						<span>正文</span>
						<Input
							value={content}
							placeholder="补充细节…"
							onChange={(e) => setContent(e.target.value)}
						/>
					</label>
				</div>
			</Modal>
		</>
	);
}
