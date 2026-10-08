/**
 * 赛事控制台 · 数据大屏标签（`events.getData(eventId)` → `DataPresent`）。
 *
 * 只渲染后端返回的聚合字段：参赛人数 / 战队数、挂题解出统计、Top10 积分榜、
 * 走势（`TrendItem[]`）与最近解出流水。30s 轮询（与平台积分榜 / 趋势语义一致）。
 */

import type { ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { Activity, BarChart3, Puzzle, Users } from "lucide-react";

import { call } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill } from "~/components/app/badges";
import { TrendChart } from "~/components/app/charts";
import type { DataTableColumn } from "~/components/app/data-table";
import { DataTable } from "~/components/app/data-table";
import { MonoText, SectionCard, StatCard } from "~/components/app/page";
import { EmptyBlock, QueryState, TableSkeleton } from "~/components/app/states";
import { UserAvatar } from "~/components/app/user-cell";
import type { DataEventChallenge, DataEventChallengeSolve, DataPresent, ScoreboardItem } from "@floatctf/sdk";
import { formatDateTime, formatPercent, formatRelative, formatScore } from "~/lib/format";
import { useNow } from "~/lib/hooks";

import { MobileFacts } from "./shared";

export function EventDataTab({ eventId }: { eventId: string }): ReactNode {
	const client = useClient();
	const now = useNow(30_000);

	const query = useQuery({
		queryKey: qk.admin.eventData(eventId),
		queryFn: () => call<DataPresent>(client.admin.events.getData(eventId), "赛事数据大屏"),
		refetchInterval: 30_000,
		refetchIntervalInBackground: false,
	});

	return (
		<QueryState
			query={query}
			errorTitle="加载赛事数据失败"
			skeleton={<TableSkeleton rows={6} columns={4} />}
		>
			{(data) => <DataPresentView data={data} now={now} />}
		</QueryState>
	);
}

function DataPresentView({ data, now }: { data: DataPresent; now: Date }): ReactNode {
	const challengeColumns: DataTableColumn<DataEventChallenge>[] = [
		{ id: "name", header: "题目", cell: (row) => row.name, sortValue: (row) => row.name },
		{
			id: "category",
			header: "分类",
			cell: (row) => <TonePill tone="neutral">{row.category}</TonePill>,
			sortValue: (row) => row.category,
		},
		{
			id: "points",
			header: "当前分值",
			align: "right",
			cell: (row) => <MonoText>{formatScore(row.points)}</MonoText>,
			sortValue: (row) => row.points,
		},
		{
			id: "solved_count",
			header: "解出数",
			align: "right",
			cell: (row) => <MonoText>{row.solved_count}</MonoText>,
			sortValue: (row) => row.solved_count,
		},
		{
			id: "solved_percent",
			header: "解出率",
			align: "right",
			cell: (row) => <MonoText>{formatPercent(row.solved_percent)}</MonoText>,
			sortValue: (row) => row.solved_percent,
		},
	];

	const scoreboardColumns: DataTableColumn<ScoreboardItem>[] = [
		{
			id: "no",
			header: "#",
			align: "right",
			cell: (row) => <MonoText>{row.no}</MonoText>,
			sortValue: (row) => row.no,
		},
		{
			id: "name",
			header: "选手 / 战队",
			cell: (row) => (
				<div className="flex items-center gap-2">
					<UserAvatar name={row.name} avatar={row.avatar} size="sm" />
					<span className="truncate">{row.name}</span>
				</div>
			),
			sortValue: (row) => row.name,
		},
		{
			id: "score",
			header: "总分",
			align: "right",
			cell: (row) => <MonoText>{formatScore(row.score)}</MonoText>,
			sortValue: (row) => row.score,
		},
		{
			id: "solved_count",
			header: "解题数",
			align: "right",
			cell: (row) => <MonoText>{row.solved_count}</MonoText>,
			sortValue: (row) => row.solved_count,
			hideBelow: "md",
		},
	];

	const solveColumns: DataTableColumn<DataEventChallengeSolve>[] = [
		{
			id: "user",
			header: "选手",
			cell: (row) => row.user_nickname || "—",
			sortValue: (row) => row.user_nickname,
		},
		{
			id: "challenge",
			header: "题目",
			cell: (row) => (
				<span>
					<span className="text-muted-foreground">{row.challenge_category} / </span>
					{row.challenge_name}
				</span>
			),
			sortValue: (row) => row.challenge_name,
		},
		{
			id: "created_at",
			header: "解出时间",
			cell: (row) => (
				<span className="tnum text-xs" title={formatDateTime(row.created_at, { seconds: true })}>
					{formatRelative(row.created_at, now)}
				</span>
			),
			sortValue: (row) => row.created_at,
		},
	];

	const trendSeries = data.trend.map((item) => ({
		name: item.name,
		points: item.points.map((point) => ({ x: Date.parse(point.time), y: point.score })),
	}));

	const solvedTotal = data.event_challenges.reduce((sum, item) => sum + item.solved_count, 0);

	return (
		<div className="space-y-5">
			<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
				<StatCard
					label="参赛人数"
					value={data.user_count}
					hint="event_users 记录数"
					icon={<Users className="size-5" />}
				/>
				<StatCard
					label="战队数"
					value={data.team_count}
					hint="个人赛恒为 0"
					icon={<Users className="size-5" />}
					tone="success"
				/>
				<StatCard
					label="挂题数"
					value={data.event_challenges.length}
					hint={`累计解出 ${solvedTotal} 次`}
					icon={<Puzzle className="size-5" />}
				/>
				<StatCard
					label="最近解出"
					value={data.solved_recent_15.length}
					hint="最近 15 条流水"
					icon={<Activity className="size-5" />}
					tone="warning"
				/>
			</div>

			<SectionCard
				title="总分走势"
				description="后端按解出流水的总分快照生成；悬停可查看各主体分数。"
			>
				<TrendChart
					series={trendSeries}
					formatY={(value) => value.toFixed(0)}
					formatX={(value) =>
						new Date(value).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", hour12: false })
					}
				/>
			</SectionCard>

			<div className="grid gap-5 lg:grid-cols-2">
				<SectionCard title="积分榜 Top10" description="与选手端积分榜同源（banned=false 过滤）。">
					{data.scoreboard_top10.length === 0 ? (
						<EmptyBlock title="暂无排行数据" description="还没有队伍 / 选手得分。" />
					) : (
						<DataTable
							data={data.scoreboard_top10}
							columns={scoreboardColumns}
							getRowId={(row) => row.id}
							mobileCard={(row) => (
								<MobileFacts
									items={[
										{ k: "#", v: <MonoText>{row.no}</MonoText> },
										{ k: "名称", v: row.name },
										{ k: "总分", v: <MonoText>{formatScore(row.score)}</MonoText> },
									]}
								/>
							)}
						/>
					)}
				</SectionCard>

				<SectionCard title="题目解出统计" description="按解出数倒序（后端排序）。">
					{data.event_challenges.length === 0 ? (
						<EmptyBlock title="赛事还没有题目" description="挂题后这里会显示解出率与动态分值。" />
					) : (
						<DataTable
							data={data.event_challenges}
							columns={challengeColumns}
							getRowId={(row) => `${row.category}-${row.name}`}
							mobileCard={(row) => (
								<MobileFacts
									items={[
										{ k: "题目", v: row.name },
										{ k: "分类", v: <TonePill tone="neutral">{row.category}</TonePill> },
										{ k: "当前分值", v: <MonoText>{formatScore(row.points)}</MonoText> },
										{ k: "解出", v: <MonoText>{`${row.solved_count} 次 / ${formatPercent(row.solved_percent)}`}</MonoText> },
									]}
								/>
							)}
						/>
					)}
				</SectionCard>
			</div>

			<SectionCard
				title="最近解出流水"
				description="最近 15 条解题记录（含额外加分）。"
			>
				{data.solved_recent_15.length === 0 ? (
					<EmptyBlock
						title="暂无解题流水"
						description="还没有选手解出赛事题目。"
						icon={<BarChart3 className="size-5" />}
					/>
				) : (
					<DataTable
						data={data.solved_recent_15}
						columns={solveColumns}
						getRowId={(row) => `${row.user_nickname}-${row.challenge_name}-${row.created_at}`}
						mobileCard={(row) => (
							<MobileFacts
								items={[
									{ k: "选手", v: row.user_nickname || "—" },
									{ k: "题目", v: row.challenge_name },
									{ k: "加分", v: <MonoText>{formatScore(row.bonus_points)}</MonoText> },
									{ k: "时间", v: formatDateTime(row.created_at) },
								]}
							/>
						)}
					/>
				)}
			</SectionCard>
		</div>
	);
}
