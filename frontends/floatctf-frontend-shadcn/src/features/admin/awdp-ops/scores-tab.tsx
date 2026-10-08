/**
 * 「积分榜」标签：AWDP 管理端积分榜（个人 / 战队主体）。
 *
 * 已核实的后端语义：`GET /admin/events/{id}/awdp/scores` 与选手端同一套服务，
 * 返回 `AwdpScoreRow[]`（subject_id / subject_name / break_score / fix_score / total_score / rank），
 * 主体是个人还是战队由赛事 `participant_mode` 决定（这里只如实展示“主体”）。
 */

import { RefreshCw } from "lucide-react";

import type { AwdpScoreRow } from "@floatctf/sdk";

import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { MonoText, SectionCard } from "~/components/app/page";
import { EmptyBlock, QueryState, TableSkeleton } from "~/components/app/states";
import { Button } from "~/components/ui/button";
import { formatScore } from "~/lib/format";

import { useAwdpScores } from "./queries";

export function AwdpScoresTab({ eventId }: { eventId: string }) {
	const scoresQuery = useAwdpScores(eventId);

	const columns: DataTableColumn<AwdpScoreRow>[] = [
		{
			id: "rank",
			header: "排名",
			align: "right",
			sortValue: (row) => row.rank,
			cell: (row) => <MonoText>#{row.rank}</MonoText>,
		},
		{
			id: "subject_name",
			header: "主体",
			sortValue: (row) => row.subject_name,
			cell: (row) => <span className="font-medium">{row.subject_name}</span>,
		},
		{
			id: "break_score",
			header: "攻破分",
			align: "right",
			sortValue: (row) => row.break_score,
			cell: (row) => <MonoText>{formatScore(row.break_score, 0)}</MonoText>,
		},
		{
			id: "fix_score",
			header: "修复分",
			align: "right",
			sortValue: (row) => row.fix_score,
			cell: (row) => <MonoText>{formatScore(row.fix_score, 0)}</MonoText>,
		},
		{
			id: "total_score",
			header: "总分",
			align: "right",
			sortValue: (row) => row.total_score,
			cell: (row) => <MonoText className="font-semibold">{formatScore(row.total_score, 0)}</MonoText>,
		},
		{
			id: "subject_id",
			header: "主体 ID",
			hideBelow: "lg",
			cell: (row) => <MonoText>{row.subject_id}</MonoText>,
		},
	];

	return (
		<SectionCard
			title="积分榜"
			description="每 30 秒自动刷新；断线或阶段切换后可用刷新按钮立即取最新值。"
			actions={
				<Button
					variant="ghost"
					size="sm"
					disabled={scoresQuery.isFetching}
					onClick={() => void scoresQuery.refetch()}
				>
					<RefreshCw /> 刷新
				</Button>
			}
		>
			<QueryState
				query={scoresQuery}
				skeleton={<TableSkeleton rows={6} columns={5} />}
				isEmpty={(rows) => rows.length === 0}
				empty={<EmptyBlock title="暂无积分数据" description="比赛开始并产生计分事件后这里才有排名。" />}
			>
				{(rows) => (
					<DataTable
						data={rows}
						getRowId={(row) => row.subject_id}
						columns={columns}
						mobileCard={(row) => (
							<div className="flex items-center justify-between gap-2">
								<div className="min-w-0">
									<p className="truncate text-sm font-medium">
										#{row.rank} {row.subject_name}
									</p>
									<p className="text-xs text-muted-foreground">
										攻破 {formatScore(row.break_score, 0)} · 修复{" "}
										{formatScore(row.fix_score, 0)}
									</p>
								</div>
								<MonoText className="font-semibold">{formatScore(row.total_score, 0)}</MonoText>
							</div>
						)}
					/>
				)}
			</QueryState>
		</SectionCard>
	);
}
