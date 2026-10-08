/**
 * 「积分与判罚」标签：AWD 积分榜 + 审计化的分数调整。
 *
 * 已核实的后端语义：
 * - `scores` 返回 `AwdScoreRow[]`（rank / attack_score / defense_score / total_score）；
 * - `adjustScore` 追加一条 `ScoreEventType::Adjustment` 账本事件（delta 有符号，正数加分），
 *   并写入 `ScoreAdjusted` 审计；赛事进入终态（finished / archived）后被拒绝。
 */

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Scale } from "lucide-react";

import type { AwdScoreRow } from "@floatctf/sdk";

import { call, callVoid } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { Field } from "~/components/app/form";
import { MonoText, SectionCard } from "~/components/app/page";
import { EmptyBlock, InlineError, QueryState, TableSkeleton } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "~/components/ui/select";
import { formatDelta, formatScore } from "~/lib/format";

import { useAwdStatus, useAwdTeams } from "./queries";

const TERMINAL_STATUSES = ["finished", "archived"];

export function AwdScoresTab({ eventId }: { eventId: string }) {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const statusQuery = useAwdStatus(eventId);
	const teamsQuery = useAwdTeams(eventId);

	const [teamId, setTeamId] = useState("");
	const [delta, setDelta] = useState("");
	const [reason, setReason] = useState("");

	const scoresQuery = useQuery({
		queryKey: qk.awd.adminScores(eventId),
		queryFn: () => call<AwdScoreRow[]>(client.awd.admin.scores(eventId), "AWD 积分榜"),
		enabled: eventId !== "",
	});

	const status = statusQuery.data?.status ?? null;
	const locked = status !== null && TERMINAL_STATUSES.includes(status);

	const bannedTeams = new Set(
		(teamsQuery.data ?? []).filter((item) => item.team.banned).map((item) => item.team.id),
	);
	const teamName = (id: string) =>
		(teamsQuery.data ?? []).find((item) => item.team.id === id)?.team.name ?? null;

	const adjust = useMutation({
		mutationFn: (input: { team_id: string; delta: number; reason: string }) =>
			callVoid(client.awd.admin.adjustScore(eventId, input), "调整分数"),
		onSuccess: (_result, input) => {
			toast.success(
				"分数已调整",
				`${teamName(input.team_id) ?? input.team_id} ${formatDelta(input.delta, 0)} 分；已记入审计。`,
			);
			void queryClient.invalidateQueries({ queryKey: qk.awd.adminScores(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.awd.adminStatus(eventId) });
			setDelta("");
			setReason("");
		},
		onError: (error) => toast.apiError("调整分数失败", error),
	});

	const columns: DataTableColumn<AwdScoreRow>[] = [
		{
			id: "rank",
			header: "排名",
			align: "right",
			sortValue: (row) => row.rank,
			cell: (row) => <MonoText>#{row.rank}</MonoText>,
		},
		{
			id: "team",
			header: "战队",
			sortValue: (row) => row.team_name,
			cell: (row) => (
				<span className="inline-flex items-center gap-2">
					<span>{row.team_name}</span>
					{bannedTeams.has(row.team_id) ? <TonePill tone="danger">已封禁</TonePill> : null}
				</span>
			),
		},
		{
			id: "attack_score",
			header: "攻击分",
			align: "right",
			sortValue: (row) => row.attack_score,
			cell: (row) => <MonoText>{formatScore(row.attack_score, 0)}</MonoText>,
		},
		{
			id: "defense_score",
			header: "防守分",
			align: "right",
			sortValue: (row) => row.defense_score,
			cell: (row) => <MonoText>{formatScore(row.defense_score, 0)}</MonoText>,
		},
		{
			id: "total_score",
			header: "总分",
			align: "right",
			sortValue: (row) => row.total_score,
			cell: (row) => <MonoText className="font-semibold">{formatScore(row.total_score, 0)}</MonoText>,
		},
		{
			id: "team_id",
			header: "战队 ID",
			hideBelow: "lg",
			cell: (row) => <MonoText>{row.team_id}</MonoText>,
		},
	];

	const deltaValue = delta.trim() === "" ? null : Number(delta);
	const deltaValid =
		deltaValue !== null && Number.isFinite(deltaValue) && Number.isInteger(deltaValue);
	const canSubmit =
		!locked && teamId !== "" && deltaValid && deltaValue !== 0 && reason.trim() !== "" && !adjust.isPending;

	return (
		<div className="space-y-5">
			<SectionCard
				title="分数调整（记入审计）"
				description="delta 为有符号整数：正数加分、负数扣分；每次调整都会写入积分账本与审计日志。"
			>
				<div className="space-y-3">
					<div className="grid gap-4 sm:grid-cols-3">
						<Field label="战队" htmlFor="awd-adjust-team" required>
							<Select value={teamId} onValueChange={setTeamId} disabled={locked}>
								<SelectTrigger id="awd-adjust-team" aria-label="选择战队">
									<SelectValue placeholder="选择战队…" />
								</SelectTrigger>
								<SelectContent>
									{(teamsQuery.data ?? []).map((item) => (
										<SelectItem key={item.team.id} value={item.team.id}>
											{item.team.name}
											{item.team.banned ? "（已封禁）" : ""}
										</SelectItem>
									))}
								</SelectContent>
							</Select>
						</Field>
						<Field
							label="分数增减"
							htmlFor="awd-adjust-delta"
							required
							hint="整数，例如 100 或 -50"
							error={delta.trim() !== "" && !deltaValid ? "必须是整数" : undefined}
						>
							<Input
								id="awd-adjust-delta"
								type="number"
								inputMode="numeric"
								value={delta}
								disabled={locked}
								onChange={(event) => setDelta(event.target.value)}
								placeholder="100"
							/>
						</Field>
						<Field label="原因" htmlFor="awd-adjust-reason" required hint="会写入审计与账本备注">
							<Input
								id="awd-adjust-reason"
								value={reason}
								disabled={locked}
								onChange={(event) => setReason(event.target.value)}
								placeholder="例如：判题误判补偿"
							/>
						</Field>
					</div>
					{teamsQuery.isPending ? (
						<p className="text-xs text-muted-foreground">加载战队列表…</p>
					) : teamsQuery.isError ? (
						<InlineError error={teamsQuery.error} />
					) : null}
					{locked ? (
						<p className="text-xs text-destructive">
							赛事已处于终态（{status}），后端拒绝再调整分数。
						</p>
					) : null}
					<div className="flex items-center gap-2">
						<Button
							size="sm"
							disabled={!canSubmit}
							title={
								locked
									? "赛事已进入终态，不可调整分数"
									: "选择战队、填写非零整数与原因后可提交"
							}
							onClick={async () => {
								if (deltaValue === null) return;
								const ok = await confirm({
									title: `确认调整分数？（${formatDelta(deltaValue, 0)}）`,
									description: "该操作会追加一条积分账本事件并写入审计日志。",
									consequences: [
										`战队「${teamName(teamId) ?? teamId}」总分直接变动 ${formatDelta(deltaValue, 0)}`,
										"调整以账本事件记录，不会被自动回滚（如需撤销请再调整一次）",
										"审计中记录操作管理员、赛事、战队与 delta",
									],
									tone: "danger",
									confirmText: "应用调整",
								});
								if (ok) {
									adjust.mutate({
										team_id: teamId,
										delta: deltaValue,
										reason: reason.trim(),
									});
								}
							}}
						>
							<Scale /> 应用调整
						</Button>
						<span className="text-xs text-muted-foreground">
							调整后会立即失效积分榜缓存并重新拉取。
						</span>
					</div>
				</div>
			</SectionCard>

			<SectionCard title="积分榜" description="按总分排名（后端 rank 字段）。">
				<QueryState
					query={scoresQuery}
					skeleton={<TableSkeleton rows={6} columns={5} />}
					isEmpty={(rows) => rows.length === 0}
					empty={<EmptyBlock title="暂无积分数据" description="开赛后产生了得分事件，这里才会有排名。" />}
				>
					{(rows) => (
						<DataTable
							data={rows}
							getRowId={(row) => row.team_id}
							columns={columns}
							mobileCard={(row) => (
								<div className="flex items-center justify-between gap-2">
									<div className="min-w-0">
										<p className="truncate text-sm font-medium">
											#{row.rank} {row.team_name}
										</p>
										<p className="text-xs text-muted-foreground">
											攻 {formatScore(row.attack_score, 0)} · 防{" "}
											{formatScore(row.defense_score, 0)}
										</p>
									</div>
									<MonoText className="font-semibold">{formatScore(row.total_score, 0)}</MonoText>
								</div>
							)}
						/>
					)}
				</QueryState>
			</SectionCard>
		</div>
	);
}
