/**
 * 练习题目详情 + 提交。
 *
 * 流程（与赛事内提交共用同一条语义，但入口不同）：
 *   启动实例 → 拿到连接信息（`instance.content`）→ 提交 flag（按 `instance_id` 计分）。
 *
 * 安全取舍：**不渲染 `instance.flag`** —— 对选手它就是答案。
 * 只展示 `content`（连接信息 / 静态题说明）。
 */
import { useState } from "react";
import { useNavigate, useParams } from "react-router";
import {
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
	CheckCircleIcon,
	DownloadIcon,
	FlagIcon,
	PlayIcon,
	RefreshIcon,
} from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { PageHeader } from "~/app/shell";
import { EmptyState, ErrorState } from "~/components/states";
import { formatTime } from "~/lib/event";

export function ChallengeDetail() {
	const { challengeId = "" } = useParams();
	const runtime = useRuntime();
	const navigate = useNavigate();
	const [flag, setFlag] = useState("");

	const challenge = useUniQuery(
		["island", "challenge", challengeId],
		() => runtime.client.service.challenges.get(challengeId),
		"加载题目",
	);

	const launch = useUniMutation(
		() => runtime.client.service.instances.launch(challengeId),
		"启动实例",
	);
	const submit = useUniMutation(
		(vars: { instance_id: string; flag: string }) => runtime.client.service.submit.submit(vars),
		"提交 flag",
	);

	if (challenge.isLoading) return <Loading active tip="加载题目…" />;
	if (challenge.error) {
		return (
			<ErrorState error={challenge.error} onRetry={() => void challenge.refetch()} />
		);
	}
	if (!challenge.data) {
		return (
			<Card>
				<EmptyState
					title="题目不存在"
					description="它可能已被删除，或者尚未公开。"
					action={
						<Button onClick={() => void navigate("/island/challenges")}>回题库</Button>
					}
				/>
			</Card>
		);
	}

	const c = challenge.data;
	const solved = Boolean(c.solved);
	const instance = launch.data ?? null;

	async function doSubmit() {
		const trimmed = flag.trim();
		if (!trimmed) return;
		try {
			const inst = instance ?? (await launch.mutateAsync(undefined));
			await submit.mutateAsync({ instance_id: inst.id, flag: trimmed });
			Notification.success({ message: "提交成功", description: "这一题算你的了。" });
			setFlag("");
			void challenge.refetch();
		} catch {
			// 失败已由 mutationCache 统一提示；保留输入让用户改。
		}
	}

	return (
		<>
			<PageHeader
				title={c.name}
				description={`分类 ${c.category}｜版本 ${c.version ?? "—"}`}
				extra={
					<>
						<Button
							size="small"
							icon={<ArrowLeftIcon size={16} />}
							onClick={() => void navigate("/island/challenges")}
						>
							回题库
						</Button>
						<Button
							size="small"
							icon={<RefreshIcon size={16} />}
							onClick={() => {
								void challenge.refetch();
								void launch.reset();
							}}
						>
							刷新
						</Button>
					</>
				}
			/>

			<Card>
				<div className="ai-col" style={{ gap: 10 }}>
					<div className="ai-row" style={{ gap: 8 }}>
						<Tag size="small" variant="outlined">
							{c.category}
						</Tag>
						{solved ? (
							<Tag size="small" variant="solid" color="app-green">
								已解出
							</Tag>
						) : (
							<Tag size="small" variant="soft" color="app-yellow">
								未解出
							</Tag>
						)}
						{c.attachment ? (
							<Button
								size="small"
								type="default"
								icon={<DownloadIcon size={16} />}
								onClick={() =>
									window.open(
										`/public/challenges/${c.safe_name}/${c.attachment?.name}`,
										"_blank",
										"noopener",
									)
								}
							>
								{c.attachment.name}
							</Button>
						) : null}
					</div>
					<Divider />
					<div className="ai-col" style={{ gap: 4 }}>
						<span className="ai-strong">题目描述</span>
						<pre style={{ margin: 0, whiteSpace: "pre-wrap", fontFamily: "inherit" }}>
							{c.description?.trim() || "（这题没有写描述）"}
						</pre>
					</div>
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<div className="ai-row-between">
						<span className="ai-strong">解题环境</span>
						<Tooltip title="需要容器的题会为你启动一个独立实例">
							<span className="ai-muted" style={{ fontSize: 12 }}>
								{c.container_port ? "容器题" : "静态题"}
							</span>
						</Tooltip>
					</div>

					{launch.error ? (
						<ErrorState error={launch.error} onRetry={() => void launch.mutate(undefined)} />
					) : null}

					{instance ? (
						<div className="ai-col" style={{ gap: 8 }}>
							<div className="ai-row" style={{ gap: 8 }}>
								<Tag size="small" variant="soft" color="app-teal">
									{instance.status}
								</Tag>
								{instance.destroy_at ? (
									<span className="ai-muted" style={{ fontSize: 12 }}>
										将在 {formatTime(instance.destroy_at)} 回收
									</span>
								) : null}
							</div>
							{instance.content ? (
								<div className="ai-terminal">{instance.content}</div>
							) : (
								<span className="ai-muted">
									实例已就绪，但没有返回可展示的连接信息。
								</span>
							)}
						</div>
					) : (
						<div className="ai-row">
							<Button
								type="primary"
								icon={<PlayIcon size={16} />}
								loading={launch.isPending}
								onClick={() => void launch.mutate(undefined)}
							>
								启动实例
							</Button>
							<span className="ai-muted">提交时也会自动启动，这里可以提前起。</span>
						</div>
					)}
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 10 }}>
					<span className="ai-strong">提交 flag</span>
					{solved ? (
						<div className="ai-row" style={{ gap: 8 }}>
							<CheckCircleIcon size={18} />
							<span>你已经解出这一题了。重复提交不会重复计分。</span>
						</div>
					) : (
						<div className="ai-row" style={{ gap: 8 }}>
							<div style={{ flex: 1, minWidth: 240 }}>
								<Input
									allowClear
									placeholder="flag{...}"
									prefix={<FlagIcon size={16} />}
									value={flag}
									onChange={(e) => setFlag(e.target.value)}
									onKeyDown={(e) => {
										if (e.key === "Enter") void doSubmit();
									}}
								/>
							</div>
							<Button
								type="primary"
								icon={<FlagIcon size={16} />}
								loading={submit.isPending || launch.isPending}
								disabled={!flag.trim()}
								onClick={() => void doSubmit()}
							>
								提交
							</Button>
						</div>
					)}
				</div>
			</Card>
		</>
	);
}
