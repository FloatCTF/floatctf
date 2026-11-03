/**
 * 题解（Writeup）：公开题解浏览 + 我自己的题解撰写。
 *
 * 两个分区而不是一个列表，因为两件事的意图完全不同：
 * 读别人的题解是**学习**，写自己的题解是**记录**；
 * 后者必须先选一道**我已解出**的题（未解出的题不该有题解入口）。
 *
 * 题解正文用等宽字体块展示：这是代码/命令语境，符合设计法里
 * "等宽字体只用于代码"的例外条款（`CodeBlock` 与终端同理）。
 */
import { useEffect, useState } from "react";
import {
	Avatar,
	Button,
	Card,
	Divider,
	Drawer,
	Input,
	Loading,
	Notification,
	Select,
	Tabs,
	Tag,
} from "animal-island-ui";
import { BookIcon, EditIcon, EyeIcon, SaveIcon, SearchIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { PageHeader } from "~/app/shell";
import { EmptyState, ErrorState, SkeletonCards } from "~/components/states";
import { formatTime } from "~/lib/event";

/** 公开题解：列表 + 抽屉阅读。 */
function PublicWriteups() {
	const runtime = useRuntime();
	const [keyword, setKeyword] = useState("");
	const [openId, setOpenId] = useState<string | null>(null);

	const list = useUniQuery(
		["island", "writeups"],
		() => runtime.client.service.challenges.getAllWriteups(),
		"加载题解",
	);
	const detail = useUniQuery(
		["island", "writeup", openId ?? ""],
		() => runtime.client.service.challenges.getWriteup(openId ?? ""),
		"加载题解正文",
		{ enabled: Boolean(openId) },
	);

	const rows = (list.data ?? []).filter((w) => {
		const k = keyword.trim().toLowerCase();
		if (!k) return true;
		return `${w.content_name}${w.nickname}`.toLowerCase().includes(k);
	});

	if (list.isLoading) return <SkeletonCards count={4} />;
	if (list.error) return <ErrorState error={list.error} onRetry={() => void list.refetch()} />;

	return (
		<div className="ai-col" style={{ gap: 12 }}>
			<Input
				allowClear
				prefix={<SearchIcon size={16} />}
				placeholder="搜索题目或作者"
				value={keyword}
				onChange={(e) => setKeyword(e.target.value)}
			/>

			{rows.length === 0 ? (
				<EmptyState
					title={keyword ? "没有匹配的题解" : "还没有公开题解"}
					description={keyword ? "换个关键词试试。" : "解出题目后可以写下你的思路。"}
				/>
			) : (
				<div className="ai-grid">
					{rows.map((w) => (
						<Card key={`${w.writeup_type}-${w.id}`} hoverable>
							<div className="ai-col" style={{ gap: 8 }}>
								<div className="ai-row-between">
									<span className="ai-strong">{w.content_name}</span>
									<Tag size="small" variant="outlined">
										{w.writeup_type === "challenge" ? "题目" : "AWDP"}
									</Tag>
								</div>
								<div className="ai-row" style={{ gap: 8 }}>
									<Avatar size="small" src={w.avatar ?? undefined}>
										{w.nickname?.[0] ?? "?"}
									</Avatar>
									<span className="ai-muted">{w.nickname}</span>
								</div>
								<Divider />
								<div className="ai-row-between">
									<span className="ai-muted" style={{ fontSize: 12 }}>
										更新于 {formatTime(w.updated_at)}
									</span>
									<Button
										size="small"
										type="primary"
										icon={<EyeIcon size={14} />}
										onClick={() => setOpenId(w.id)}
									>
										阅读
									</Button>
								</div>
							</div>
						</Card>
					))}
				</div>
			)}

			<Drawer
				open={openId !== null}
				onClose={() => setOpenId(null)}
				title={detail.data?.content_name ?? "题解"}
				placement="right"
				width={560}
			>
				{detail.isLoading ? (
					<Loading active tip="加载正文…" />
				) : detail.error ? (
					<ErrorState error={detail.error} onRetry={() => void detail.refetch()} />
				) : detail.data ? (
					<div className="ai-col" style={{ gap: 10 }}>
						<div className="ai-row" style={{ gap: 8 }}>
							<Avatar size="small" src={detail.data.avatar ?? undefined}>
								{detail.data.nickname?.[0] ?? "?"}
							</Avatar>
							<span className="ai-strong">{detail.data.nickname}</span>
							{detail.data.category ? (
								<Tag size="small" variant="outlined">
									{detail.data.category}
								</Tag>
							) : null}
						</div>
						<span className="ai-muted" style={{ fontSize: 12 }}>
							更新于 {formatTime(detail.data.updated_at)}
						</span>
						<Divider />
						{/* 题解正文属于代码/命令语境，用等宽块展示是刻意的例外。 */}
						<div className="ai-terminal" style={{ maxHeight: "none" }}>
							{detail.data.content}
						</div>
					</div>
				) : (
					<EmptyState title="题解不存在" />
				)}
			</Drawer>
		</div>
	);
}

/** 我的题解：先选一道已解出的题，再写/改。 */
function MyWriteups() {
	const runtime = useRuntime();
	const [challengeId, setChallengeId] = useState("");
	const [content, setContent] = useState("");
	const [loadedFor, setLoadedFor] = useState<string | null>(null);

	const challenges = useUniQuery(
		["island", "challenges"],
		() => runtime.client.service.challenges.fetch(),
		"加载题目",
	);
	// 只有解出过的题才允许写题解：未解出就写等于抄答案。
	const solved = (challenges.data ?? []).filter((c) => c.solved);

	const mine = useUniQuery(
		["island", "my-writeup", challengeId],
		() => runtime.client.service.challenges.getMyWriteup(challengeId),
		"加载我的题解",
		{ enabled: Boolean(challengeId) },
	);

	// 切换题目时把已有正文灌进编辑器（只灌一次，避免覆盖用户正在输入的内容）。
	useEffect(() => {
		if (!challengeId) return;
		if (loadedFor === challengeId) return;
		if (mine.data === undefined) return;
		setContent(mine.data?.content ?? "");
		setLoadedFor(challengeId);
	}, [challengeId, mine.data, loadedFor]);

	const save = useUniMutation(
		(vars: { challenge_id: string; content: string }) =>
			runtime.client.service.challenges.createMyWriteup(vars),
		"保存题解",
	);

	if (challenges.isLoading) return <SkeletonCards count={2} />;
	if (challenges.error) {
		return (
			<ErrorState error={challenges.error} onRetry={() => void challenges.refetch()} />
		);
	}

	if (solved.length === 0) {
		return (
			<EmptyState
				title="你还没有解出任何题目"
				description="解出一道题之后，就可以在这里记录你的思路。"
			/>
		);
	}

	return (
		<div className="ai-col" style={{ gap: 12 }}>
			<span className="ai-muted">
				只能为自己**已解出**的题目写题解 —— 未解出的题目不会出现在这里。
			</span>

			<div style={{ maxWidth: 360 }}>
				<Select
					value={challengeId}
					onChange={setChallengeId}
					placeholder="选择一道你解出的题"
					showSearch
					options={solved.map((c) => ({ key: c.id, label: c.name }))}
				/>
			</div>

			{!challengeId ? (
				<EmptyState title="先选一道题" description="选择后可以撰写或修改你的题解。" />
			) : mine.isLoading ? (
				<Loading active tip="加载我的题解…" />
			) : mine.error ? (
				<ErrorState error={mine.error} onRetry={() => void mine.refetch()} />
			) : (
				<div className="ai-col" style={{ gap: 10 }}>
					{mine.data ? (
						<Tag size="small" variant="soft" color="app-green">
							已有题解，更新于 {formatTime(mine.data.updated_at)}
						</Tag>
					) : (
						<Tag size="small" variant="soft" color="app-yellow">
							还没有题解
						</Tag>
					)}
					<Input
						value={content}
						placeholder="写下解题思路…（支持纯文本，代码可以直接粘贴）"
						onChange={(e) => setContent(e.target.value)}
					/>
					<div className="ai-row" style={{ justifyContent: "flex-end" }}>
						<Button
							type="primary"
							icon={<SaveIcon size={16} />}
							loading={save.isPending}
							disabled={!content.trim()}
							onClick={async () => {
								await save.mutateAsync({ challenge_id: challengeId, content: content.trim() });
								Notification.success({ message: "题解已保存" });
								setLoadedFor(null);
								void mine.refetch();
							}}
						>
							{save.isPending ? "保存中…" : "保存题解"}
						</Button>
					</div>
				</div>
			)}
		</div>
	);
}

export function WriteupList() {
	return (
		<>
			<PageHeader
				title="题解"
				description="读别人的思路，或者记录自己的。点开公开题解会在侧边抽屉里展开正文。"
				extra={
					<Tag size="small" variant="soft" color="app-teal">
						<BookIcon size={12} /> 学习与记录
					</Tag>
				}
			/>
			<Card>
				<Tabs
					aria-label="题解分区"
					defaultActiveKey="public"
					items={[
						{ key: "public", label: "公开题解", children: <PublicWriteups /> },
						{
							key: "mine",
							label: (
								<span className="ai-row" style={{ gap: 4 }}>
									<EditIcon size={14} /> 我的题解
								</span>
							),
							children: <MyWriteups />,
						},
					]}
				/>
			</Card>
		</>
	);
}
