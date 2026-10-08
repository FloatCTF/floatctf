/**
 * 赛事控制台 · AWD / AWDP 只读概览标签。
 *
 * 这里**只做摘要与跳转**：完整的运维流程（部署 / 开始 / 暂停 / 结束 / 归档 / 预检 /
 * GameBox 与网络管理）属于 `features/admin/awd-ops` 与 `features/admin/awdp-ops` 的独立页面，
 * 本标签不复制那套操作，避免出现两个「能改状态」的入口。
 *
 * - AWD：`client.awd.admin.getStatus(eventId)`（`data` 可能为 `null` → `callMaybe`）+ `scores`
 * - AWDP：`client.awdp.admin.getConfig(eventId)` + `scores`
 */

import type { ReactNode } from "react";
import { Link } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { ExternalLink, RefreshCw, ShieldAlert } from "lucide-react";

import { call, callMaybe } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill, type PillTone } from "~/components/app/badges";
import type { DataTableColumn } from "~/components/app/data-table";
import { DataTable } from "~/components/app/data-table";
import { KeyValueList, MonoText, SectionCard, StatCard, Toolbar } from "~/components/app/page";
import { EmptyBlock, ErrorBlock, LoadingBlock, QueryState } from "~/components/app/states";
import { Button } from "~/components/ui/button";
import type { AwdpEventConfigDto, AwdpScoreRow, AwdEventStatus, AwdScoreRow } from "@floatctf/sdk";
import { formatDateTime, formatDuration, formatScore } from "~/lib/format";

import { MobileFacts } from "./shared";

/* ── AWD ────────────────────────────────────────────────────────────────── */

function awdStatusTone(status: string): PillTone {
	switch (status) {
		case "running":
		case "verified":
			return "success";
		case "paused":
		case "start_blocked":
		case "verification_failed":
		case "deploy_failed":
			return "warning";
		case "network_error":
			return "danger";
		case "configuring":
		case "deploying":
		case "deployed":
		case "prechecking":
			return "info";
		default:
			return "muted";
	}
}

const AWD_PHASE_NOTE: Record<string, string> = {
	hardening: "加固阶段",
	attack: "攻击阶段",
	pause: "已暂停（管理员操作）",
};

export function EventAwdTab({ eventId }: { eventId: string }): ReactNode {
	const client = useClient();
	const statusQuery = useQuery({
		queryKey: qk.awd.adminStatus(eventId),
		queryFn: () => callMaybe<AwdEventStatus>(client.awd.admin.getStatus(eventId), "AWD 状态"),
	});
	const scoresQuery = useQuery({
		queryKey: qk.awd.adminScores(eventId),
		queryFn: () => call<AwdScoreRow[]>(client.awd.admin.scores(eventId), "AWD 积分"),
	});

	const scoreColumns: DataTableColumn<AwdScoreRow>[] = [
		{
			id: "rank",
			header: "#",
			align: "right",
			cell: (row) => <MonoText>{row.rank}</MonoText>,
			sortValue: (row) => row.rank,
		},
		{ id: "team_name", header: "战队", cell: (row) => row.team_name, sortValue: (row) => row.team_name },
		{
			id: "attack_score",
			header: "攻击分",
			align: "right",
			cell: (row) => <MonoText>{formatScore(row.attack_score)}</MonoText>,
			sortValue: (row) => row.attack_score,
		},
		{
			id: "defense_score",
			header: "防守分",
			align: "right",
			cell: (row) => <MonoText>{formatScore(row.defense_score)}</MonoText>,
			sortValue: (row) => row.defense_score,
		},
		{
			id: "total_score",
			header: "总分",
			align: "right",
			cell: (row) => <MonoText>{formatScore(row.total_score)}</MonoText>,
			sortValue: (row) => row.total_score,
		},
	];

	const status = statusQuery.data ?? null;

	return (
		<div className="space-y-5">
			<SectionCard
				title="AWD 运行时摘要"
				description="只读快照；部署 / 开始 / 暂停 / 结束 / 归档 / 预检请在 AWD 运维页操作。"
				actions={
					<Toolbar>
						<Button
							variant="outline"
							size="sm"
							disabled={statusQuery.isFetching}
							onClick={() => {
								void statusQuery.refetch();
								void scoresQuery.refetch();
							}}
						>
							<RefreshCw className={statusQuery.isFetching ? "animate-spin" : undefined} /> 刷新
						</Button>
						<Button size="sm" asChild>
							<Link to={`/admin/events/${eventId}/awd`}>
								<ExternalLink /> 打开 AWD 运维页
							</Link>
						</Button>
					</Toolbar>
				}
			>
				{statusQuery.isPending ? (
					<LoadingBlock label="加载 AWD 状态…" />
				) : statusQuery.isError ? (
					<ErrorBlock
						error={statusQuery.error}
						title="加载 AWD 状态失败"
						onRetry={() => void statusQuery.refetch()}
					/>
				) : status === null ? (
					<EmptyBlock
						title="该赛事尚未配置 AWD"
						description="后端返回空状态：还没有 awd_events 配置行。请在 AWD 运维页创建配置。"
						icon={<ShieldAlert className="size-5" />}
						action={
							<Button size="sm" asChild>
								<Link to={`/admin/events/${eventId}/awd`}>前往 AWD 运维页</Link>
							</Button>
						}
					/>
				) : (
					<div className="space-y-5">
						<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
							<StatCard
								label="状态"
								value={<TonePill tone={awdStatusTone(status.status)}>{status.status}</TonePill>}
								hint="后端 awd_events.status 原值"
							/>
							<StatCard
								label="阶段"
								value={<TonePill tone="info">{status.phase}</TonePill>}
								hint={AWD_PHASE_NOTE[status.phase]}
							/>
							<StatCard
								label="轮次数"
								value={status.round_count === null ? "未设置" : status.round_count}
								hint={`单轮 ${formatDuration(status.round_duration_secs)}`}
							/>
							<StatCard
								label="初始分"
								value={formatScore(status.initial_score)}
								hint={`免费重置 ${status.free_reset_count} 次 · 额外重置罚 ${formatScore(status.extra_reset_penalty)}`}
								tone="warning"
							/>
						</div>

						{status.status === "network_error" ? (
							<p className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm text-destructive">
								后端报告 network_error：平台基础设施异常，比赛已暂停，需管理员在运维页恢复。
							</p>
						) : null}
						{status.final_settlement ? (
							<p className="rounded-md border border-[var(--warning)]/40 bg-[var(--warning)]/10 px-3 py-2 text-sm">
								最终结算中（final_settlement）：Judge 正在结算最后一轮，比赛动作已关闭。
							</p>
						) : null}

						<KeyValueList
							columns={3}
							items={[
								{
									key: "计划开始时间",
									value: status.planned_start_at ? (
										<MonoText>{formatDateTime(status.planned_start_at, { seconds: true })}</MonoText>
									) : (
										<span className="text-xs text-muted-foreground">未设置</span>
									),
								},
								{
									key: "实际开始时间",
									value: status.started_at ? (
										<MonoText>{formatDateTime(status.started_at, { seconds: true })}</MonoText>
									) : (
										<span className="text-xs text-muted-foreground">未开始</span>
									),
								},
								{
									key: "预检通过时间",
									value: status.verified_at ? (
										<MonoText>{formatDateTime(status.verified_at, { seconds: true })}</MonoText>
									) : (
										<span className="text-xs text-muted-foreground">未通过校验</span>
									),
								},
								{
									key: "Judge 并发 / 超时",
									value: (
										<MonoText>
											{status.judge_max_concurrency} 并发 · {formatDuration(status.judge_default_timeout_secs)} 超时
										</MonoText>
									),
								},
								{
									key: "Judge 重试间隔",
									value: <MonoText>{formatDuration(status.judge_retry_interval_secs)}</MonoText>,
								},
								{
									key: "归档保留",
									value: <MonoText>{status.archive_retention_hours} 小时</MonoText>,
								},
								{
									key: "状态更新时间",
									value: <MonoText>{formatDateTime(status.updated_at, { seconds: true })}</MonoText>,
								},
							]}
						/>
					</div>
				)}
			</SectionCard>

			<SectionCard title="AWD 积分榜" description="后端 `awd.admin.scores` 的原始排名。">
				<QueryState
					query={scoresQuery}
					errorTitle="加载 AWD 积分失败"
					skeleton={<LoadingBlock label="加载 AWD 积分…" />}
					isEmpty={(rows) => rows.length === 0}
					empty={<EmptyBlock title="暂无 AWD 积分" description="比赛开始并产生得分后这里会有数据。" />}
				>
					{(rows) => (
						<DataTable
							data={rows}
							columns={scoreColumns}
							getRowId={(row) => row.team_id}
							mobileCard={(row) => (
								<MobileFacts
									items={[
										{ k: "#", v: <MonoText>{row.rank}</MonoText> },
										{ k: "战队", v: row.team_name },
										{ k: "攻击分", v: <MonoText>{formatScore(row.attack_score)}</MonoText> },
										{ k: "防守分", v: <MonoText>{formatScore(row.defense_score)}</MonoText> },
										{ k: "总分", v: <MonoText>{formatScore(row.total_score)}</MonoText> },
									]}
								/>
							)}
						/>
					)}
				</QueryState>
			</SectionCard>
		</div>
	);
}

/* ── AWDP ───────────────────────────────────────────────────────────────── */

/** `AwdpPhase` 含过渡态 `preparing_fix`，必须如实展示（见 CONVENTIONS §6.19）。 */
const AWDP_PHASE_LABEL: Record<string, string> = {
	pending: "待开始",
	break: "Break（攻击）",
	preparing_fix: "准备修复（过渡态）",
	fix: "Fix（修复）",
	ended: "已结束",
};

function awdpPhaseTone(phase: string): PillTone {
	switch (phase) {
		case "break":
			return "danger";
		case "preparing_fix":
			return "warning";
		case "fix":
			return "info";
		case "ended":
			return "muted";
		default:
			return "neutral";
	}
}

export function EventAwdpTab({ eventId }: { eventId: string }): ReactNode {
	const client = useClient();
	const configQuery = useQuery({
		queryKey: qk.awdp.adminConfig(eventId),
		queryFn: () => call<AwdpEventConfigDto>(client.awdp.admin.getConfig(eventId), "AWDP 配置"),
	});
	const scoresQuery = useQuery({
		queryKey: qk.awdp.adminScores(eventId),
		queryFn: () => call<AwdpScoreRow[]>(client.awdp.admin.scores(eventId), "AWDP 积分"),
	});

	const scoreColumns: DataTableColumn<AwdpScoreRow>[] = [
		{
			id: "rank",
			header: "#",
			align: "right",
			cell: (row) => <MonoText>{row.rank}</MonoText>,
			sortValue: (row) => row.rank,
		},
		{
			id: "subject_name",
			header: "选手 / 战队",
			cell: (row) => row.subject_name,
			sortValue: (row) => row.subject_name,
		},
		{
			id: "break_score",
			header: "Break 分",
			align: "right",
			cell: (row) => <MonoText>{formatScore(row.break_score)}</MonoText>,
			sortValue: (row) => row.break_score,
		},
		{
			id: "fix_score",
			header: "Fix 分",
			align: "right",
			cell: (row) => <MonoText>{formatScore(row.fix_score)}</MonoText>,
			sortValue: (row) => row.fix_score,
		},
		{
			id: "total_score",
			header: "总分",
			align: "right",
			cell: (row) => <MonoText>{formatScore(row.total_score)}</MonoText>,
			sortValue: (row) => row.total_score,
		},
	];

	return (
		<div className="space-y-5">
			<SectionCard
				title="AWDP 配置摘要"
				description="只读配置与阶段快照；阶段推进（start / break-to-fix / finish）与 GameBox 管理请在 AWDP 工作台操作。"
				actions={
					<Toolbar>
						<Button
							variant="outline"
							size="sm"
							disabled={configQuery.isFetching}
							onClick={() => {
								void configQuery.refetch();
								void scoresQuery.refetch();
							}}
						>
							<RefreshCw className={configQuery.isFetching ? "animate-spin" : undefined} /> 刷新
						</Button>
						<Button size="sm" asChild>
							<Link to={`/admin/events/${eventId}/awdp`}>
								<ExternalLink /> 打开 AWDP 工作台
							</Link>
						</Button>
					</Toolbar>
				}
			>
				<QueryState
					query={configQuery}
					errorTitle="加载 AWDP 配置失败"
					skeleton={<LoadingBlock label="加载 AWDP 配置…" />}
				>
					{(config) => (
						<div className="space-y-5">
							<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
								<StatCard
									label="阶段"
									value={<TonePill tone={awdpPhaseTone(config.phase)}>{config.phase}</TonePill>}
									hint={AWDP_PHASE_LABEL[config.phase] ?? "后端 phase 原值"}
								/>
								<StatCard
									label="当前轮次"
									value={`${config.current_round} / ${config.total_rounds}`}
									hint={`配置代数 ${config.configuration_generation}`}
								/>
								<StatCard
									label="Break 时长 / 分值"
									value={formatDuration(config.break_duration_secs)}
									hint={`每题 ${formatScore(config.break_score)} 分`}
								/>
								<StatCard
									label="Fix 时长 / 分值"
									value={formatDuration(config.fix_duration_secs)}
									hint={`每轮 ${formatScore(config.fix_round_score)} 分 · 轮间隔 ${formatDuration(config.fix_round_interval_secs)}`}
									tone="success"
								/>
							</div>

							<KeyValueList
								columns={3}
								items={[
									{
										key: "开始时间",
										value: config.started_at ? (
											<MonoText>{formatDateTime(config.started_at, { seconds: true })}</MonoText>
										) : (
											<span className="text-xs text-muted-foreground">未开始</span>
										),
									},
									{
										key: "Break 结束时间",
										value: config.break_ends_at ? (
											<MonoText>{formatDateTime(config.break_ends_at, { seconds: true })}</MonoText>
										) : (
											<span className="text-xs text-muted-foreground">—</span>
										),
									},
									{
										key: "Fix 开始 / 结束",
										value: (
											<MonoText>
												{config.fix_started_at
													? formatDateTime(config.fix_started_at, { seconds: true })
													: "—"}{" "}
												→{" "}
												{config.fix_ends_at
													? formatDateTime(config.fix_ends_at, { seconds: true })
													: "—"}
											</MonoText>
										),
									},
									{
										key: "结束时间",
										value: config.finished_at ? (
											<MonoText>{formatDateTime(config.finished_at, { seconds: true })}</MonoText>
										) : (
											<span className="text-xs text-muted-foreground">未结束</span>
										),
									},
									{
										key: "下一步动作时间",
										value: config.next_action_at ? (
											<MonoText>{formatDateTime(config.next_action_at, { seconds: true })}</MonoText>
										) : (
											<span className="text-xs text-muted-foreground">—</span>
										),
									},
									{
										key: "配置更新时间",
										value: <MonoText>{formatDateTime(config.updated_at, { seconds: true })}</MonoText>,
									},
								]}
							/>
						</div>
					)}
				</QueryState>
			</SectionCard>

			<SectionCard title="AWDP 积分榜" description="后端 `awdp.admin.scores` 的原始排名。">
				<QueryState
					query={scoresQuery}
					errorTitle="加载 AWDP 积分失败"
					skeleton={<LoadingBlock label="加载 AWDP 积分…" />}
					isEmpty={(rows) => rows.length === 0}
					empty={<EmptyBlock title="暂无 AWDP 积分" description="Break / Fix 计分产生后这里会有数据。" />}
				>
					{(rows) => (
						<DataTable
							data={rows}
							columns={scoreColumns}
							getRowId={(row) => row.subject_id}
							mobileCard={(row) => (
								<MobileFacts
									items={[
										{ k: "#", v: <MonoText>{row.rank}</MonoText> },
										{ k: "选手 / 战队", v: row.subject_name },
										{ k: "Break 分", v: <MonoText>{formatScore(row.break_score)}</MonoText> },
										{ k: "Fix 分", v: <MonoText>{formatScore(row.fix_score)}</MonoText> },
										{ k: "总分", v: <MonoText>{formatScore(row.total_score)}</MonoText> },
									]}
								/>
							)}
						/>
					)}
				</QueryState>
			</SectionCard>
		</div>
	);
}
