/**
 * AWDP 练习（训练）目录。
 *
 * 练习与正式比赛用的是**不同的 API 面**（`awdp.runs` vs `awdp.player`），
 * 因为练习的计分不入正式榜、可以随意重开。界面上也据此区分措辞：
 * 这里所有的分数都是"练习分"，不会出现在任何比赛的排行榜里。
 *
 * 「继续训练」与「开始训练」是两个不同的动作：
 * `gameboxCatalog` 会带上当前用户在该箱子上的 `active_training`，
 * 因此有进行中的练习时要引导用户**回到**它，而不是开一个新的（否则旧的会变成孤儿 run）。
 */
import { useMemo, useState } from "react";
import { useNavigate } from "react-router";
import { Button, Card, Divider, Input, Notification, Tag } from "animal-island-ui";
import { MagnetIcon, PlayIcon, RefreshIcon, SearchIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { PageHeader } from "~/app/shell";
import { EmptyState, ErrorState, SkeletonCards } from "~/components/states";
import { StatCard } from "~/components/resource";
import { formatTime } from "~/lib/event";

function phaseLabel(phase: string | undefined): { label: string; color: string } {
	switch (phase) {
		case "break":
			return { label: "Break（攻破）", color: "app-red" };
		case "preparing_fix":
			return { label: "准备修复", color: "app-yellow" };
		case "fix":
			return { label: "Fix（修复）", color: "app-green" };
		case "ended":
			return { label: "已结束", color: "brown" };
		default:
			return { label: phase ?? "—", color: "brown" };
	}
}

export function AwdpPractice() {
	const runtime = useRuntime();
	const navigate = useNavigate();
	const [keyword, setKeyword] = useState("");

	const catalog = useUniQuery(
		["awdp-catalog"],
		() => runtime.client.awdp.runs.gameboxCatalog(),
		"加载练习目录",
	);
	const start = useUniMutation(
		(vars: string) => runtime.client.awdp.runs.startTraining(vars),
		"开始训练",
	);

	const rows = useMemo(() => {
		const list = catalog.data ?? [];
		const k = keyword.trim().toLowerCase();
		if (!k) return list;
		return list.filter((g) => `${g.name}${g.description}${g.category}`.toLowerCase().includes(k));
	}, [catalog.data, keyword]);

	if (catalog.isLoading) return <SkeletonCards count={4} />;
	if (catalog.error) {
		return <ErrorState error={catalog.error} onRetry={() => void catalog.refetch()} />;
	}

	const all = catalog.data ?? [];
	const active = all.filter((g) => g.active_training);
	const capable = all.filter((g) => g.awdp_capable);

	return (
		<>
			<PageHeader
				title="AWDP 练习"
				description="随时开始的漏洞修复练习。练习分数独立于任何比赛，不会进入排行榜。"
				extra={
					<Button
						size="small"
						icon={<RefreshIcon size={16} />}
						onClick={() => void catalog.refetch()}
					>
						刷新
					</Button>
				}
			/>

			<div className="ai-grid">
				<StatCard label="可练习箱子" value={capable.length} icon={<MagnetIcon size={18} />} />
				<StatCard label="进行中" value={active.length} hint="你有未结束的练习" />
			</div>

			{active.length > 0 ? (
				<Card>
					<div className="ai-col" style={{ gap: 10 }}>
						<span className="ai-strong">你有进行中的练习</span>
						<span className="ai-muted" style={{ fontSize: 12 }}>
							建议先回到这些练习继续，而不是重复开始 —— 每个箱子同时只应有一个练习。
						</span>
						<Divider />
						{active.map((g) => {
							const t = g.active_training;
							const meta = phaseLabel(t?.phase);
							return (
								<div key={g.id} className="ai-row-between">
									<div className="ai-row" style={{ gap: 8 }}>
										<span className="ai-strong">{g.name}</span>
										<Tag size="small" variant="solid" color={meta.color as "app-green"}>
											{meta.label}
										</Tag>
										<Tag size="small" variant="outlined">
											{t?.score ?? 0} 分
										</Tag>
									</div>
									<Button
										size="small"
										type="primary"
										icon={<PlayIcon size={14} />}
										onClick={() => void navigate(`/island/awdp/runs/${t?.run_id}`)}
									>
										继续
									</Button>
								</div>
							);
						})}
					</div>
				</Card>
			) : null}

			<Card>
				<Input
					allowClear
					prefix={<SearchIcon size={16} />}
					placeholder="搜索箱子名称、分类或描述"
					value={keyword}
					onChange={(e) => setKeyword(e.target.value)}
				/>
			</Card>

			{rows.length === 0 ? (
				<Card>
					<EmptyState
						title={keyword ? "没有匹配的箱子" : "还没有可练习的箱子"}
						description={
							keyword
								? "换个关键词试试。"
								: "AWDP 练习需要有支持 AWDP 的箱子。请联系管理员导入。"
						}
					/>
				</Card>
			) : (
				<div className="ai-grid">
					{rows.map((g) => (
						<Card key={g.id} hoverable>
							<div className="ai-col" style={{ gap: 8 }}>
								<div className="ai-row-between">
									<span className="ai-strong">{g.name}</span>
									<Tag size="small" variant="outlined">
										{g.category}
									</Tag>
								</div>
								{!g.awdp_capable ? (
									<Tag size="small" variant="soft" color="app-red">
										不支持 AWDP
									</Tag>
								) : null}
								<span className="ai-muted" style={{ fontSize: 13 }}>
									{g.description || "（没有描述）"}
								</span>
								<span className="ai-muted" style={{ fontSize: 12 }}>
									{g.version ? `版本 ${g.version}` : ""}
									{g.author ? ` ｜ ${g.author}` : ""}
									{g.updated_at ? ` ｜ 更新于 ${formatTime(g.updated_at)}` : ""}
								</span>
								<Divider />
								<div className="ai-row" style={{ justifyContent: "flex-end" }}>
									<Button
										type="primary"
										size="small"
										icon={<PlayIcon size={14} />}
										disabled={!g.awdp_capable || Boolean(g.active_training)}
										loading={start.isPending}
										onClick={async () => {
											const run = await start.mutateAsync(g.id);
											Notification.success({ message: "练习已创建" });
											void catalog.refetch();
											if (run?.run_id) void navigate(`/island/awdp/runs/${run.run_id}`);
										}}
									>
										{g.active_training ? "已有进行中的练习" : "开始训练"}
									</Button>
								</div>
							</div>
						</Card>
					))}
				</div>
			)}
		</>
	);
}
