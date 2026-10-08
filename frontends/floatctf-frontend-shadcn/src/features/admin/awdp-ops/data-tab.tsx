/**
 * 「数据大屏」标签（optional）：`GET /admin/events/{id}/awdp/data` 的聚合视图。
 *
 * 该标签只在被选中时挂载（Radix Tabs 默认不渲染非激活内容），因此不会给页面首屏加请求。
 * 数据全部来自后端聚合（人数 / 战队数、每题攻破与修复计数、Top10、趋势、最近动态）。
 */

import { RefreshCw } from "lucide-react";

import type { AwdpDataGameBox, AwdpScoreRow } from "@floatctf/sdk";

import { TrendChart } from "~/components/app/charts";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { MonoText, SectionCard, StatCard } from "~/components/app/page";
import { EmptyBlock, QueryState, TableSkeleton } from "~/components/app/states";
import { Button } from "~/components/ui/button";
import { formatInt, formatRelative, formatScore } from "~/lib/format";
import { useNow } from "~/lib/hooks";

import { useAwdpDataPresent } from "./queries";

export function AwdpDataTab({ eventId }: { eventId: string }) {
	const dataQuery = useAwdpDataPresent(eventId, true);
	const now = useNow(30_000);

	const gameboxColumns: DataTableColumn<AwdpDataGameBox>[] = [
		{
			id: "name",
			header: "GameBox",
			sortValue: (row) => row.name,
			cell: (row) => (
				<div className="min-w-0">
					<p className="truncate font-medium">{row.name}</p>
					<p className="text-xs text-muted-foreground">{row.category}</p>
				</div>
			),
		},
		{
			id: "break_count",
			header: "被攻破次数",
			align: "right",
			sortValue: (row) => row.break_count,
			cell: (row) => <MonoText>{formatInt(row.break_count)}</MonoText>,
		},
		{
			id: "fix_count",
			header: "修复次数",
			align: "right",
			sortValue: (row) => row.fix_count,
			cell: (row) => <MonoText>{formatInt(row.fix_count)}</MonoText>,
		},
	];

	const topColumns: DataTableColumn<AwdpScoreRow>[] = [
		{
			id: "rank",
			header: "排名",
			align: "right",
			cell: (row) => <MonoText>#{row.rank}</MonoText>,
		},
		{
			id: "subject_name",
			header: "主体",
			cell: (row) => row.subject_name,
		},
		{
			id: "total_score",
			header: "总分",
			align: "right",
			cell: (row) => <MonoText className="font-semibold">{formatScore(row.total_score, 0)}</MonoText>,
		},
	];

	return (
		<QueryState
			query={dataQuery}
			skeleton={<TableSkeleton rows={5} columns={4} />}
			errorTitle="加载赛事大屏数据失败"
		>
			{(data) => (
				<div className="space-y-5">
					<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
						<StatCard label="参与人数" value={formatInt(data.user_count)} />
						<StatCard label="战队数" value={formatInt(data.team_count)} />
						<StatCard
							label="题目数"
							value={formatInt(data.gameboxes.length)}
							hint="已挂载的 GameBox"
						/>
						<StatCard
							label="最近动态"
							value={formatInt(data.recent_activity.length)}
							hint="后端返回的最新计分事件条数"
						/>
					</div>

					<SectionCard
						title="Top10 积分"
						description="与积分榜同源，按总分排序。"
						actions={
							<Button
								variant="ghost"
								size="sm"
								disabled={dataQuery.isFetching}
								onClick={() => void dataQuery.refetch()}
							>
								<RefreshCw /> 刷新
							</Button>
						}
					>
						{data.scoreboard_top10.length === 0 ? (
							<EmptyBlock title="暂无积分数据" />
						) : (
							<DataTable
								data={data.scoreboard_top10}
								getRowId={(row) => row.subject_id}
								columns={topColumns}
							/>
						)}
					</SectionCard>

					<SectionCard title="每题攻防统计" description="被攻破次数与修复成功次数。">
						{data.gameboxes.length === 0 ? (
							<EmptyBlock title="暂无题目统计" />
						) : (
							<DataTable
								data={data.gameboxes}
								getRowId={(row) => row.id}
								columns={gameboxColumns}
							/>
						)}
					</SectionCard>

					<SectionCard title="得分趋势" description="后端聚合的分数时间序列。">
						<TrendChart
							series={data.trend.map((item) => ({
								name: item.name,
								points: item.points.map((point) => ({
									x: Date.parse(point.time),
									y: point.score,
								})),
							}))}
							formatY={(value) => formatScore(value, 0)}
							formatX={(value) =>
								new Date(value).toLocaleTimeString("zh-CN", { hour12: false })
							}
						/>
					</SectionCard>

					<SectionCard title="最近计分动态" description="最新的攻破与修复事件。">
						{data.recent_activity.length === 0 ? (
							<EmptyBlock title="暂无计分动态" />
						) : (
							<ul className="divide-y">
								{data.recent_activity.map((activity, index) => (
									<li
										key={`${activity.subject_name}-${activity.gamebox_name}-${activity.created_at}-${index}`}
										className="flex flex-wrap items-center justify-between gap-2 py-2.5"
									>
										<div className="min-w-0">
											<p className="truncate text-sm">
												<span className="font-medium">{activity.subject_name}</span>
												<span className="text-muted-foreground">
													{activity.action === "break" ? " 攻破 " : " 修复 "}
												</span>
												<span>{activity.gamebox_name}</span>
											</p>
											<p className="text-xs text-muted-foreground">
												{activity.gamebox_category} · {formatRelative(activity.created_at, now)}
											</p>
										</div>
										<span className="tnum font-mono text-sm">
											{activity.delta > 0 ? "+" : ""}
											{formatScore(activity.delta, 0)}
										</span>
									</li>
								))}
							</ul>
						)}
					</SectionCard>
				</div>
			)}
		</QueryState>
	);
}
