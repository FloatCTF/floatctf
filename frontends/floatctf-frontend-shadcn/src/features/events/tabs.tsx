/**
 * 赛事工作区的标签内容。每个标签只做「真实接口 → 展示 / 操作」，
 * 状态文案统一走 `~/lib/event-status.ts` 与后端返回字段，不前端自造。
 */

import { useState } from "react";
import { Link } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckCircle2, ChevronRight, ExternalLink, Swords, Trash2, Trophy } from "lucide-react";

import { call, callList, callVoid } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TrendChart } from "~/components/app/charts";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { MarkdownView } from "~/components/app/markdown";
import {
	KeyValueList,
	MonoText,
	PaginationBar,
	SectionCard,
	StatCard,
} from "~/components/app/page";
import { EmptyBlock, LoadingBlock, QueryState, TableSkeleton } from "~/components/app/states";
import { TonePill } from "~/components/app/badges";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import type {
	AwdpOverview,
	AwdPlayerStatus,
	EventChallengeResult,
	EventInfo,
	EventInstanceResult,
	ScoreboardItem,
	TrendItem,
} from "@floatctf/sdk";
import type { EventAnnouncements } from "@floatctf/sdk/entity";
import { formatDateTime, formatRelative, formatScore } from "~/lib/format";
import { useNow } from "~/lib/hooks";

import { EventFactsCard, EventWriteupPanel } from "./components";

/* ── 总览 ───────────────────────────────────────────────────────────────── */

export function OverviewTab({ info }: { info: EventInfo }) {
	return (
		<div className="grid gap-5 lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
			<div className="space-y-5">
				<SectionCard title="赛事说明" description="平台与主办方发布的信息。">
					<MarkdownView emptyText="主办方未填写赛事说明。">{info.event.description}</MarkdownView>
				</SectionCard>
				<SectionCard title="规则" description="请务必在解题前阅读。">
					<MarkdownView emptyText="未填写规则。">{info.event.rules}</MarkdownView>
				</SectionCard>
			</div>
			<div className="space-y-5">
				<EventFactsCard event={info.event} />
				<SectionCard title="赛后材料" description="比赛结束后提交 writeup（PDF）。">
					<EventWriteupPanel info={info} />
				</SectionCard>
			</div>
		</div>
	);
}

/* ── 赛事题目 ───────────────────────────────────────────────────────────── */

export function ChallengesTab({ eventId }: { eventId: string }) {
	const client = useClient();
	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(20);

	const query = useQuery({
		queryKey: qk.events.challenges(eventId, { limit: 200 }),
		queryFn: () => callList(client.service.events.fetchChallenges(eventId, { limit: 200 })),
	});

	const items = query.data?.items ?? [];
	const sorted = [...items].sort((a, b) => b.current_points - a.current_points);
	const paged = sorted.slice((page - 1) * pageSize, page * pageSize);

	const columns: DataTableColumn<EventChallengeResult>[] = [
		{
			id: "name",
			header: "题目",
			sortValue: (row) => row.challenge.name,
			cell: (row) => (
				<div className="min-w-0">
					<Link
						to={`/events/${eventId}/challenges/${row.challenge.id}`}
						className="truncate text-sm font-medium hover:underline"
					>
						{row.challenge.name}
					</Link>
					<p className="truncate text-xs text-muted-foreground">
						{row.challenge.category || "未分类"}
					</p>
				</div>
			),
		},
		{
			id: "points",
			header: "当前分值",
			align: "right",
			sortValue: (row) => row.current_points,
			cell: (row) => <span className="tnum font-mono text-xs">{formatScore(row.current_points)}</span>,
		},
		{
			id: "solves",
			header: "解出队伍",
			align: "right",
			hideBelow: "sm",
			sortValue: (row) => row.solved_count,
			cell: (row) => <span className="tnum font-mono text-xs">{row.solved_count}</span>,
		},
		{
			id: "solved",
			header: "我的状态",
			cell: (row) =>
				row.solved ? (
					<TonePill tone="success" icon={<CheckCircle2 />}>
						已解出 #{row.solved_no}
					</TonePill>
				) : (
					<TonePill tone="muted">未解出</TonePill>
				),
		},
		{
			id: "action",
			header: "",
			align: "right",
			cell: (row) => (
				<Button variant="ghost" size="sm" asChild>
					<Link to={`/events/${eventId}/challenges/${row.challenge.id}`}>
						打开 <ChevronRight />
					</Link>
				</Button>
			),
		},
	];

	return (
		<SectionCard
			title="赛事题目"
			description={query.isSuccess ? `共 ${items.length} 道已发布题目` : undefined}
			contentClassName="px-0"
			footer={
				items.length > pageSize ? (
					<PaginationBar
						page={page}
						pageSize={pageSize}
						total={items.length}
						onPageChange={setPage}
						onPageSizeChange={(size) => {
							setPageSize(size);
							setPage(1);
						}}
					/>
				) : null
			}
		>
			<QueryState
				query={query}
				skeleton={<TableSkeleton rows={6} columns={5} />}
				isEmpty={() => items.length === 0}
				empty={
					<EmptyBlock
						title="还没有开放题目"
						description="主办方发布题目后会出现在这里；比赛开始前可能全部隐藏。"
					/>
				}
			>
				{() => (
					<DataTable
						data={paged}
						getRowId={(row) => row.challenge.id}
						columns={columns}
						mobileCard={(row) => (
							<div className="flex items-center justify-between gap-2">
								<div className="min-w-0">
									<p className="truncate text-sm font-medium">{row.challenge.name}</p>
									<p className="text-xs text-muted-foreground">
										{formatScore(row.current_points)} 分 · {row.solved_count} 队解出
									</p>
								</div>
								<Link to={`/events/${eventId}/challenges/${row.challenge.id}`}>
									<ChevronRight className="size-4 text-muted-foreground" />
								</Link>
							</div>
						)}
					/>
				)}
			</QueryState>
		</SectionCard>
	);
}

/* ── 积分榜 ─────────────────────────────────────────────────────────────── */

export function ScoreboardTab({ info }: { info: EventInfo }) {
	const client = useClient();
	const query = useQuery({
		queryKey: qk.events.scoreboard(info.event.id),
		queryFn: () => callList(client.service.events.getScoreboard(info.event.id)),
		refetchInterval: 30_000,
	});

	const myName =
		info.event.participant_mode === "team"
			? (info.team_result?.team.name ?? null)
			: null;

	const columns: DataTableColumn<ScoreboardItem>[] = [
		{
			id: "rank",
			header: "#",
			align: "right",
			sortValue: (row) => row.no,
			cell: (row) => <span className="tnum font-mono text-xs">{row.no}</span>,
		},
		{
			id: "name",
			header: "选手 / 战队",
			sortValue: (row) => row.name,
			cell: (row) => (
				<div className="flex min-w-0 items-center gap-2">
					<span className="truncate text-sm font-medium">{row.name}</span>
					{myName && row.name === myName ? <TonePill tone="info">我的战队</TonePill> : null}
				</div>
			),
		},
		{
			id: "solved",
			header: "解题数",
			align: "right",
			sortValue: (row) => row.solved_count,
			cell: (row) => <span className="tnum font-mono text-xs">{row.solved_count}</span>,
		},
		{
			id: "score",
			header: "总分",
			align: "right",
			sortValue: (row) => row.score,
			cell: (row) => <span className="tnum font-mono text-sm">{formatScore(row.score)}</span>,
		},
		{
			id: "grid",
			header: "题目矩阵",
			hideBelow: "lg",
			cell: (row) => (
				<div className="flex flex-wrap gap-1">
					{row.challenges.map((cell) => (
						<span
							key={cell.name}
							title={cell.solved ? `${cell.name}：第 ${cell.solved_no} 解` : `${cell.name}：未解出`}
							className={
								cell.solved
									? "inline-block size-2.5 rounded-sm bg-[var(--success)]"
									: "inline-block size-2.5 rounded-sm bg-muted"
							}
						/>
					))}
				</div>
			),
		},
	];

	return (
		<SectionCard
			title="积分榜"
			description="30s 自动刷新；比赛进行中分数可能仍在变化。"
			actions={
				<Button variant="ghost" size="sm" onClick={() => query.refetch()}>
					立即刷新
				</Button>
			}
			contentClassName="px-0"
		>
			<QueryState
				query={query}
				skeleton={<TableSkeleton rows={6} columns={4} />}
				errorTitle="加载积分榜失败"
				isEmpty={(result) => result.items.length === 0}
				empty={<EmptyBlock title="还没有计分记录" icon={<Trophy className="size-5" />} />}
			>
				{(result) => (
					<DataTable
						data={result.items}
						getRowId={(row) => row.id}
						columns={columns}
						mobileCard={(row) => (
							<div className="flex items-center justify-between gap-2">
								<div className="min-w-0">
									<p className="truncate text-sm font-medium">
										#{row.no} {row.name}
									</p>
									<p className="text-xs text-muted-foreground">{row.solved_count} 题</p>
								</div>
								<span className="tnum font-mono text-sm">{formatScore(row.score)}</span>
							</div>
						)}
					/>
				)}
			</QueryState>
		</SectionCard>
	);
}

/* ── 趋势 ───────────────────────────────────────────────────────────────── */

export function TrendTab({ eventId }: { eventId: string }) {
	const client = useClient();
	const query = useQuery({
		queryKey: qk.events.trend(eventId),
		queryFn: () => call<TrendItem[]>(client.service.events.getTrend(eventId), "分数趋势"),
		refetchInterval: 30_000,
	});

	return (
		<SectionCard title="分数趋势" description="每名选手 / 战队的总分随时间变化。">
			<QueryState
				query={query}
				skeleton={<LoadingBlock label="加载趋势…" />}
				errorTitle="加载趋势失败"
				isEmpty={(series) => series.every((item) => item.points.length === 0)}
				empty={<EmptyBlock title="还没有趋势数据" />}
			>
				{(series) => (
					<TrendChart
						series={series.map((item) => ({
							name: item.name,
							points: item.points.map((point) => ({
								x: new Date(point.time).getTime(),
								y: point.score,
							})),
						}))}
						formatY={(value) => value.toFixed(0)}
						formatX={(value) =>
							new Date(value).toLocaleTimeString("zh-CN", {
								hour: "2-digit",
								minute: "2-digit",
								hour12: false,
							})
						}
					/>
				)}
			</QueryState>
		</SectionCard>
	);
}

/* ── 赛事实例 ───────────────────────────────────────────────────────────── */

export function InstancesTab({ eventId }: { eventId: string }) {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const now = useNow(30_000);

	const query = useQuery({
		queryKey: qk.events.instances(eventId),
		queryFn: () => call<EventInstanceResult[]>(client.service.events.getInstances(eventId), "赛事实例"),
		refetchInterval: 30_000,
	});

	const destroy = useMutation({
		mutationFn: (id: string) => callVoid(client.service.instances.destroy(id), "销毁实例"),
		onSuccess: () => {
			toast.success("实例已销毁");
			void queryClient.invalidateQueries({ queryKey: qk.events.instances(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.instances.all });
		},
		onError: (error) => toast.apiError("销毁失败", error),
	});

	const columns: DataTableColumn<EventInstanceResult>[] = [
		{
			id: "challenge",
			header: "题目",
			sortValue: (row) => row.challenge_name,
			cell: (row) => <span className="text-sm">{row.challenge_name}</span>,
		},
		{
			id: "owner",
			header: "归属",
			hideBelow: "sm",
			cell: (row) => <span className="text-xs">{row.user_nickname}</span>,
		},
		{
			id: "status",
			header: "状态",
			cell: (row) => <TonePill tone="neutral">{row.instance.status || "未知"}</TonePill>,
		},
		{
			id: "identifier",
			header: "容器",
			hideBelow: "md",
			cell: (row) => <MonoText>{row.instance.identifier || "—"}</MonoText>,
		},
		{
			id: "created",
			header: "启动于",
			hideBelow: "md",
			sortValue: (row) => row.instance.created_at,
			cell: (row) => (
				<span className="text-xs text-muted-foreground">
					{formatRelative(row.instance.created_at, now)}
				</span>
			),
		},
		{
			id: "expires",
			header: "自动回收",
			hideBelow: "lg",
			cell: (row) => (
				<span className="text-xs text-muted-foreground">
					{row.instance.destroy_at ? formatDateTime(row.instance.destroy_at) : "—"}
				</span>
			),
		},
	];

	return (
		<SectionCard
			title="赛事实例"
			description="本赛事中你（或你的战队）启动的题目环境。"
			contentClassName="px-0"
		>
			<QueryState
				query={query}
				skeleton={<TableSkeleton rows={4} columns={5} />}
				errorTitle="加载赛事实例失败"
				isEmpty={(items) => items.length === 0}
				empty={
					<EmptyBlock
						title="还没有实例"
						description="在赛事题目里启动实例后会出现在这里。"
					/>
				}
			>
				{(items) => (
					<DataTable
						data={items}
						getRowId={(row) => row.id}
						columns={columns}
						rowActions={(row) => (
							<Button
								variant="ghost"
								size="sm"
								disabled={destroy.isPending}
								onClick={async () => {
									const ok = await confirm({
										title: "销毁该实例？",
										description: row.challenge_name,
										consequences: ["容器会被删除，环境内未保存的修改会丢失"],
										tone: "danger",
										confirmText: "销毁",
									});
									if (ok) destroy.mutate(row.instance.id);
								}}
							>
								<Trash2 /> 销毁
							</Button>
						)}
						mobileCard={(row) => (
							<div className="space-y-1">
								<p className="text-sm font-medium">{row.challenge_name}</p>
								<p className="text-xs text-muted-foreground">
									{row.instance.status} · {row.instance.identifier}
								</p>
							</div>
						)}
					/>
				)}
			</QueryState>
		</SectionCard>
	);
}

/* ── 赛事公告 ───────────────────────────────────────────────────────────── */

export function BulletinTab({ eventId }: { eventId: string }) {
	const client = useClient();
	const now = useNow(30_000);
	const query = useQuery({
		queryKey: qk.events.announcements(eventId),
		queryFn: () =>
			call<EventAnnouncements[]>(client.service.events.getAnnouncements(eventId), "赛事公告"),
		refetchInterval: 60_000,
	});

	return (
		<SectionCard title="赛事公告" description="主办方发布的赛程与规则变更（60s 刷新）。">
			<QueryState
				query={query}
				skeleton={<LoadingBlock label="加载公告…" />}
				errorTitle="加载公告失败"
				isEmpty={(items) => items.length === 0}
				empty={<EmptyBlock title="暂无赛事公告" />}
			>
				{(items) => (
					<ul className="space-y-3">
						{items.map((item) => (
							<li key={item.id} className="rounded-md border p-3">
								<div className="flex flex-wrap items-center justify-between gap-2">
									<p className="text-sm font-medium">{item.title}</p>
									<span className="text-xs text-muted-foreground">
										{formatRelative(item.created_at, now)}
									</span>
								</div>
								<div className="mt-2">
									<MarkdownView>{item.content}</MarkdownView>
								</div>
							</li>
						))}
					</ul>
				)}
			</QueryState>
		</SectionCard>
	);
}

/* ── AWD / AWDP 概览（只读摘要 + 进入专用工作区） ────────────────────────── */

export function ArenaSummaryTab({ eventId }: { eventId: string }) {
	const client = useClient();
	const query = useQuery({
		queryKey: qk.awd.status(eventId),
		queryFn: () =>
			call<AwdPlayerStatus>(client.awd.player.status(eventId), "AWD 赛事状态"),
		refetchInterval: 30_000,
	});

	return (
		<div className="space-y-5">
			<QueryState
				query={query}
				skeleton={<LoadingBlock label="读取 AWD 状态…" />}
				errorTitle="读取 AWD 状态失败"
			>
				{(status) => (
					<>
						<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
							<StatCard label="赛事阶段" value={status.phase || "—"} hint={status.status} />
							<StatCard
								label="当前轮次"
								value={
									status.current_round !== null
										? `${status.current_round}${status.round_count ? ` / ${status.round_count}` : ""}`
										: "—"
								}
							/>
							<StatCard
								label="我的得分"
								value={formatScore(status.score)}
								tone="success"
								icon={<Swords className="size-5" />}
							/>
							<StatCard
								label="参赛状态"
								value={status.banned ? "已被封禁" : "正常"}
								tone={status.banned ? "danger" : "default"}
								hint={status.final_settlement ? "已进入最终结算" : undefined}
							/>
						</div>
						<SectionCard
							title="AWD 驾驶舱"
							description="GameBox、WireGuard、SSH 凭据与实时积分榜都在驾驶舱里操作。"
						>
							<Button asChild>
								<Link to={`/arena/${eventId}`}>
									进入 AWD 驾驶舱 <ExternalLink />
								</Link>
							</Button>
						</SectionCard>
					</>
				)}
			</QueryState>
		</div>
	);
}

export function LabSummaryTab({ eventId }: { eventId: string }) {
	const client = useClient();
	const query = useQuery({
		queryKey: qk.awdp.overview(eventId),
		queryFn: () => call<AwdpOverview>(client.awdp.player.overview(eventId), "AWDP 总览"),
		refetchInterval: 30_000,
	});

	return (
		<div className="space-y-5">
			<QueryState
				query={query}
				skeleton={<LoadingBlock label="读取 AWDP 总览…" />}
				errorTitle="读取 AWDP 总览失败"
			>
				{(overview) => (
					<>
						<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
							<StatCard
								label="当前阶段"
								value={overview.phase}
								hint={overview.next_action_at ? `下一节点 ${formatDateTime(overview.next_action_at)}` : undefined}
							/>
							<StatCard
								label="轮次"
								value={`${overview.current_round} / ${overview.total_rounds}`}
							/>
							<StatCard label="我的得分" value={formatScore(overview.my_score)} tone="success" />
							<StatCard label="我的靶机" value={overview.gameboxes.length} />
						</div>
						<SectionCard
							title="AWDP 工作台"
							description="实例生命周期、Break 提交、补丁上传与官方评测都在工作台里。"
						>
							<Button asChild>
								<Link to={`/lab/${eventId}`}>
									进入 AWDP 工作台 <ExternalLink />
								</Link>
							</Button>
						</SectionCard>
						<SectionCard title="阶段节奏" description="后端配置的真实时长（秒）。">
							<KeyValueList
								items={[
									{ key: "Break 时长", value: `${overview.break_duration_secs}s` },
									{ key: "Fix 时长", value: `${overview.fix_duration_secs}s` },
									{ key: "轮次间隔", value: `${overview.fix_round_interval_secs}s` },
									{ key: "Break 分值", value: formatScore(overview.break_score) },
									{ key: "每轮 Fix 分值", value: formatScore(overview.fix_round_score) },
								]}
							/>
						</SectionCard>
					</>
				)}
			</QueryState>
		</div>
	);
}
