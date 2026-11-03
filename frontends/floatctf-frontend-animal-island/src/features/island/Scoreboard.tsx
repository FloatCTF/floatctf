/**
 * 排行榜。
 *
 * 列是**动态**的：计分榜每一行都带该主体在各题上的解出状态，因此表头由
 * 第一行的 `challenges` 推导。这也意味着题目集合变了以后列会自动跟着变，
 * 不需要在前端硬编码题目列表。
 */
import { useEffect, useMemo } from "react";
import { useSearchParams } from "react-router";
import { Button, Card, Divider, Select, Tabs, Tag, Tooltip, type TableColumn } from "animal-island-ui";
import { RefreshIcon, TrophyIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniQuery } from "~/api/hooks";
import { PageHeader } from "~/app/shell";
import { ResourcePage, StatCard } from "~/components/resource";
import { EmptyState, ErrorState, SkeletonCards } from "~/components/states";
import { eventIdOf, formatTime } from "~/lib/event";
import type { ScoreboardItem } from "@floatctf/sdk";

/**
 * 走势面板。
 *
 * 与 AWDP 的趋势面板同一个判断：**不引入图表库，也不手搓 SVG**（设计法明令禁止后者）。
 * 因此这里把每个主体的得分序列渲染成时序表：时间 + 当时总分 + 增量。
 * 这比一条看不出量级的折线更实用 —— 用户真正想知道的是"谁在什么时候反超了"。
 */
function TrendPanel({ eventId }: { eventId: string }) {
	const runtime = useRuntime();
	const q = useUniQuery(
		["island", "trend", eventId],
		() => runtime.client.service.events.getTrend(eventId),
		"加载走势",
		{ enabled: Boolean(eventId) },
	);

	if (q.isLoading) return <SkeletonCards count={3} />;
	if (q.error) return <ErrorState error={q.error} onRetry={() => void q.refetch()} />;
	if ((q.data ?? []).length === 0) {
		return (
			<EmptyState
				title="还没有走势数据"
				description="有人得分后，这里会出现各个主体的得分增长序列。"
			/>
		);
	}

	return (
		<div className="ai-col" style={{ gap: 12 }}>
			<span className="ai-muted" style={{ fontSize: 12 }}>
				每个主体的得分增长序列。本项目未引入图表库，因此以时序表呈现，而不是自绘图形。
			</span>
			{(q.data ?? []).map((item) => {
				const last = item.points[item.points.length - 1];
				return (
					<Card key={item.name}>
						<div className="ai-col" style={{ gap: 8 }}>
							<div className="ai-row-between">
								<span className="ai-strong">{item.name}</span>
								<div className="ai-row" style={{ gap: 8 }}>
									<Tag size="small" variant="outlined">
										{item.points.length} 次得分
									</Tag>
									<Tag size="small" variant="soft" color="app-teal">
										当前 {last?.score ?? 0} 分
									</Tag>
								</div>
							</div>
							<Divider />
							<div className="ai-scroll" style={{ maxHeight: 220 }}>
								<table style={{ width: "100%", borderCollapse: "collapse" }}>
									<thead>
										<tr>
											{["时间", "题目", "总分", "增量"].map((h) => (
												<th
													key={h}
													style={{
														textAlign: "left",
														padding: "6px 10px",
														borderBottom: "2px solid var(--animal-border-color)",
													}}
												>
													{h}
												</th>
											))}
										</tr>
									</thead>
									<tbody>
										{item.points.map((pt, i) => {
											const prev = i > 0 ? item.points[i - 1].score : 0;
											const delta = pt.score - prev;
											return (
												<tr key={`${pt.time}-${i}`}>
													<td style={{ padding: "6px 10px" }} className="ai-muted">
														{formatTime(pt.time)}
													</td>
													<td style={{ padding: "6px 10px" }}>{pt.name}</td>
													<td style={{ padding: "6px 10px" }} className="ai-strong">
														{pt.score}
													</td>
													<td style={{ padding: "6px 10px" }}>
														<Tag
															size="small"
															variant="soft"
															color={delta >= 0 ? "app-green" : "app-red"}
														>
															{delta >= 0 ? "+" : ""}
															{delta}
														</Tag>
													</td>
												</tr>
											);
										})}
									</tbody>
								</table>
							</div>
						</div>
					</Card>
				);
			})}
		</div>
	);
}

export function Scoreboard() {
	const runtime = useRuntime();
	const [params, setParams] = useSearchParams();
	const events = useUniQuery(
		["island", "events"],
		() => runtime.client.service.events.fetch(),
		"加载赛事",
	);

	const eventId = params.get("event") ?? "";
	// 默认选第一场：没有选择时不该给一张空表。
	useEffect(() => {
		if (eventId) return;
		const first = events.data?.[0];
		if (first) setParams({ event: eventIdOf(first) }, { replace: true });
	}, [eventId, events.data, setParams]);

	const board = useUniQuery(
		["island", "scoreboard", eventId],
		() => runtime.client.service.events.getScoreboard(eventId),
		"加载排行榜",
		{ enabled: Boolean(eventId) },
	);

	const columns: TableColumn<ScoreboardItem>[] = useMemo(() => {
		const first = board.data?.[0];
		const dynamic: TableColumn<ScoreboardItem>[] = (first?.challenges ?? []).map(
			(ch, idx) => ({
				title: (
					<Tooltip title={ch.name}>
						<span style={{ fontSize: 12 }}>#{idx + 1}</span>
					</Tooltip>
				),
				width: 64,
				align: "center",
				render: (_v, row) => {
					const cell = row.challenges[idx];
					if (!cell) return <span className="ai-muted">—</span>;
					return cell.solved ? (
						<Tag size="small" variant="solid" color="app-green">
							{cell.solved_no}
						</Tag>
					) : (
						<span className="ai-muted">·</span>
					);
				},
			}),
		);
		return [
			{ title: "#", dataIndex: "no", width: 60, align: "center" },
			{ title: "名称", dataIndex: "name", width: 220 },
			{
				title: "总分",
				dataIndex: "score",
				width: 100,
				align: "right",
				render: (v) => <span className="ai-strong">{String(v)}</span>,
			},
			{
				title: "解出",
				dataIndex: "solved_count",
				width: 80,
				align: "right",
			},
			...dynamic,
		];
	}, [board.data]);

	const top = board.data?.[0];
	const totalSolved = (board.data ?? []).reduce((s, r) => s + r.solved_count, 0);

	return (
		<>
			<PageHeader
				title="排行榜"
				description="按赛事查看当前得分。悬停表头的 # 编号可以看到题目名。"
				extra={
					<Button
						size="small"
						icon={<RefreshIcon size={16} />}
						onClick={() => void board.refetch()}
					>
						刷新
					</Button>
				}
			/>

			<Card>
				<div className="ai-row" style={{ gap: 12 }}>
					<div style={{ minWidth: 260 }}>
						<Select
							value={eventId}
							onChange={(v) => setParams({ event: v })}
							placeholder="选择赛事"
							showSearch
							options={(events.data ?? []).map((e) => ({
								key: eventIdOf(e),
								label: e.event.title,
							}))}
						/>
					</div>
					{board.data ? (
						<span className="ai-muted">共 {board.data.length} 个主体参与计分</span>
					) : null}
				</div>
			</Card>

			{board.data && board.data.length > 0 ? (
				<div className="ai-grid">
					<StatCard
						label="当前第一"
						value={top?.name ?? "—"}
						icon={<TrophyIcon size={18} />}
						hint={top ? `${top.score} 分` : undefined}
					/>
					<StatCard label="参与主体" value={board.data.length} />
					<StatCard label="累计解出" value={totalSolved} hint="所有主体合计" />
				</div>
			) : null}

			{!eventId ? (
				<Card>
					<EmptyState
						title="先选一场赛事"
						description="排行榜与走势都是按赛事计算的 —— 不同赛事的题目集合不同。"
					/>
				</Card>
			) : (
				<Card>
					<Tabs
						aria-label="排行与走势"
						defaultActiveKey="board"
						items={[
							{
								key: "board",
								label: "排行榜",
								children: (
									<ResourcePage<ScoreboardItem>
										columns={columns}
										rows={board.data}
										isLoading={board.isLoading}
										error={board.error}
										refetch={() => void board.refetch()}
										rowKey="id"
										emptyTitle="这场赛事还没有人得分"
										emptyDescription="等第一个提交出现，这里就有内容了。"
										disablePagination
									/>
								),
							},
							{
								key: "trend",
								label: "走势",
								children: <TrendPanel eventId={eventId} />,
							},
						]}
					/>
				</Card>
			)}
		</>
	);
}
