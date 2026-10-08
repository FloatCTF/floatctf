/**
 * `/lab/:eventId` —— AWDP 比赛工作台（阶段驱动的单页工作区）。
 *
 * 顶部：阶段步进条（含 `preparing_fix` 过渡态）+ 关键指标
 * 左：我的靶机列表 + 实例生命周期（启动 / 停止 / 重置 / 刷新）
 * 中：Break / Fix 动作区（提交 flag、上传补丁、Test Check、源码下载）
 * 右：Fix 回合与我的评测
 * 底部：积分榜矩阵 / 我的积分 / 趋势
 *
 * 实时：`useAwdpEventStream`（`lastEvent` 来自 ref，所以用 invalidate 驱动刷新）+
 * `RealtimePill`（显式呈现 auth_error）。
 */

import { useCallback, useMemo, useState, type ReactNode } from "react";
import { Link, useParams } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Database, Trophy } from "lucide-react";

import type { AwdpInstance, AwdpOverview } from "@floatctf/sdk";

import { call, callMaybe, callVoid } from "~/api/call";
import { isForbidden, isNotFound } from "~/api/errors";
import { useBindings, useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { RealtimePill, TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import {
	KeyValueList,
	PageBody,
	PageHeader,
	SectionCard,
	StatCard,
} from "~/components/app/page";
import {
	EmptyBlock,
	ErrorBlock,
	LoadingBlock,
	PermissionDeniedBlock,
} from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "~/components/ui/tabs";
import { formatDateTime, formatDuration, formatInt, formatScore } from "~/lib/format";
import { useDocumentTitle } from "~/lib/hooks";

import {
	AwdpActionPanel,
	AwdpGameboxList,
	AwdpInstancePanel,
	AwdpMyScoresPanel,
	AwdpPhaseSteps,
	AwdpRoundsEvaluationsPanel,
	AwdpScoreboardPanel,
	AwdpTrendPanel,
} from "./components";
import {
	AWDP_INSTANCE_RESET_CONSEQUENCES,
	awdpPhaseLabel,
	awdpPhaseTone,
	isAwdpNotConfigured,
} from "./domain";

/** 停止实例的真实后果（后端：官方判定需要 running 实例）。 */
const AWDP_STOP_CONSEQUENCES = [
	"实例容器会被停止：暴露的服务端点不再可用，正在进行的自检 / 评测会失败。",
	"官方回合判定需要运行中的实例；停止期间该回合不会对你的实例计分。",
	"逻辑实例与端点保留，之后可以「启动」继续（不会重建容器）。",
];

export function LabPage(): ReactNode {
	const { eventId = "" } = useParams<{ eventId: string }>();
	useDocumentTitle("AWDP 工作台 · FloatCTF");

	const client = useClient();
	const { useAwdpEventStream } = useBindings();
	const stream = useAwdpEventStream({ eventId, enabled: Boolean(eventId) });
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const [selectedId, setSelectedId] = useState<string | null>(null);

	const overviewQuery = useQuery({
		queryKey: qk.awdp.overview(eventId),
		queryFn: () => call<AwdpOverview>(client.awdp.player.overview(eventId), "AWDP 总览"),
		enabled: Boolean(eventId),
	});

	const overview = overviewQuery.data;
	const gameboxes = useMemo(() => overview?.gameboxes ?? [], [overview]);
	const selected = gameboxes.find((item) => item.id === selectedId) ?? gameboxes[0] ?? null;
	const selectedGameboxId = selected?.id ?? "";

	// 阶段为 pending / ended 时后端没有 active run，`getInstance` 会返回
	// 400（"AWDP 事件尚未开始"）——那是业务态，不该当成加载失败：这两种阶段直接不查。
	const phase = overview?.phase ?? "pending";
	const activePhase = phase === "break" || phase === "preparing_fix" || phase === "fix";
	// 赛事是否已经开始过（决定 rounds / evaluations 这两个「必须有 run」的端点是否可查）。
	const hasRun = (overview?.started_at ?? null) !== null || phase !== "pending";

	// 实例详情：key 以 qk.awdp.instance(eventId) 为前缀 + 靶机 id，因此
	// `invalidateQueries({ queryKey: qk.awdp.instance(eventId) })` 仍能前缀失效到。
	const instanceQuery = useQuery({
		queryKey: [...qk.awdp.instance(eventId), selectedGameboxId],
		queryFn: () =>
			callMaybe<AwdpInstance>(
				client.awdp.player.getInstance(eventId, selectedGameboxId),
				"实例详情",
			),
		enabled: Boolean(eventId) && Boolean(selectedGameboxId) && activePhase,
		retry: false,
	});

	// 两个来源：overview（SSE 会失效 `awdp-overview`）与 getInstance（动作后 / 手动刷新）。
	// 取 dataUpdatedAt 更新的一方 —— 否则 SSE 已刷新的 overview 会被缓存的 getInstance 覆盖。
	const instance: AwdpInstance | null =
		instanceQuery.dataUpdatedAt >= overviewQuery.dataUpdatedAt
			? (instanceQuery.data ?? null)
			: (selected?.instance ?? null);

	const refreshAll = useCallback(() => {
		for (const key of [
			qk.awdp.overview(eventId),
			qk.awdp.instance(eventId),
			qk.awdp.rounds(eventId),
			qk.awdp.evaluations(eventId),
			qk.awdp.scores(eventId),
			qk.awdp.scoreboard(eventId),
			qk.awdp.trend(eventId),
		]) {
			void queryClient.invalidateQueries({ queryKey: key });
		}
	}, [queryClient, eventId]);

	const startMutation = useMutation({
		mutationFn: () =>
			call<AwdpInstance>(
				client.awdp.player.startInstance(eventId, selectedGameboxId),
				"启动实例",
			),
		onSuccess: () => {
			toast.success("实例已启动");
			refreshAll();
		},
		onError: (error) => toast.apiError("启动实例失败", error),
	});

	const stopMutation = useMutation({
		mutationFn: () =>
			callVoid(client.awdp.player.stopInstance(eventId, selectedGameboxId), "停止实例"),
		onSuccess: () => {
			toast.success("实例已停止");
			refreshAll();
		},
		onError: (error) => toast.apiError("停止实例失败", error),
	});

	const resetMutation = useMutation({
		mutationFn: () =>
			call<AwdpInstance>(
				client.awdp.player.resetInstance(eventId, selectedGameboxId),
				"重置实例",
			),
		onSuccess: () => {
			toast.success("实例已重置为原始状态");
			refreshAll();
		},
		onError: (error) => toast.apiError("重置实例失败", error),
	});

	const busy = startMutation.isPending || stopMutation.isPending || resetMutation.isPending;

	async function askStop() {
		const ok = await confirm({
			title: `停止「${selected?.name ?? "实例"}」？`,
			description: "停止后服务端点不再可用，官方回合判定无法执行。",
			consequences: AWDP_STOP_CONSEQUENCES,
			tone: "danger",
			confirmText: "停止",
		});
		if (ok) stopMutation.mutate();
	}

	async function askReset() {
		const ok = await confirm({
			title: `重置「${selected?.name ?? "实例"}」？`,
			description: "重置会销毁容器并按原始镜像重建（pristine）。",
			consequences: AWDP_INSTANCE_RESET_CONSEQUENCES,
			tone: "danger",
			confirmText: "重置",
		});
		if (ok) resetMutation.mutate();
	}

	return (
		<PageBody>
			<PageHeader
				title="AWDP 工作台"
				description="Break 阶段拿 flag、Fix 阶段上传补丁并自检；所有状态来自后端，实时刷新。"
				breadcrumbs={
					<nav className="flex items-center gap-1.5">
						<Link to="/events" className="hover:underline">
							赛事
						</Link>
						<span>/</span>
						<Link to={`/events/${eventId}`} className="hover:underline">
							赛事工作区
						</Link>
						<span>/</span>
						<span className="text-foreground">AWDP 工作台</span>
					</nav>
				}
				badge={
					overview ? (
						<TonePill tone={awdpPhaseTone(overview.phase)}>
							{awdpPhaseLabel(overview.phase)}
						</TonePill>
					) : null
				}
				actions={
					<RealtimePill
						state={stream.connectionState}
						onRefresh={() => {
							stream.invalidateAwdp();
							refreshAll();
						}}
					/>
				}
			/>

			{overviewQuery.isPending ? (
				<LoadingBlock label="加载 AWDP 总览…" />
			) : overviewQuery.isError ? (
				isForbidden(overviewQuery.error) ? (
					<PermissionDeniedBlock description="你不是本赛事的参赛成员（或已离队），无法进入 AWDP 工作台。" />
				) : isNotFound(overviewQuery.error) || isAwdpNotConfigured(overviewQuery.error) ? (
					<EmptyBlock
						title="该赛事未启用 AWDP"
						description="本赛事没有 AWDP 赛制配置；请从赛事列表确认赛制，或联系管理员。"
						action={
							<Button variant="outline" size="sm" asChild>
								<Link to="/events">返回赛事列表</Link>
							</Button>
						}
					/>
				) : (
					<ErrorBlock
						error={overviewQuery.error}
						title="加载 AWDP 总览失败"
						onRetry={() => void overviewQuery.refetch()}
					/>
				)
			) : overview ? (
				<div className="space-y-5">
					<SectionCard
							title="阶段"
							description="阶段由后端推进；preparing_fix 是 Break→Fix 之间的过渡态，此时不可提交。"
							actions={
								overview.next_action_at ? (
									<span className="tnum text-xs text-muted-foreground">
										下一动作 {formatDateTime(overview.next_action_at, { seconds: true })}
									</span>
								) : null
							}
						>
							<AwdpPhaseSteps
								phase={overview.phase}
								currentRound={overview.current_round}
								totalRounds={overview.total_rounds}
								nextActionAt={overview.next_action_at}
							/>
							<div className="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
								<StatCard
									label="我的分数"
									value={formatScore(overview.my_score)}
									hint="Break + Fix 累计"
									icon={<Trophy className="size-5" />}
									tone="success"
								/>
								<StatCard
									label="当前回合"
									value={`${formatInt(overview.current_round)} / ${formatInt(overview.total_rounds)}`}
									hint="Fix 回合"
									icon={<Database className="size-5" />}
								/>
								<StatCard
									label="Break 时长"
									value={formatDuration(overview.break_duration_secs)}
									hint={
										overview.break_ends_at
											? `结束于 ${formatDateTime(overview.break_ends_at, { seconds: true })}`
											: "未开始"
									}
								/>
								<StatCard
									label="Fix 时长 / 回合间隔"
									value={`${formatDuration(overview.fix_duration_secs)} / ${formatDuration(overview.fix_round_interval_secs)}`}
									hint={
										overview.fix_ends_at
											? `结束于 ${formatDateTime(overview.fix_ends_at, { seconds: true })}`
											: "未开始"
									}
								/>
							</div>
							<KeyValueList
								className="mt-4"
								columns={3}
								items={[
									{
										key: "Break 单题得分",
										value: <span className="tnum">{formatScore(overview.break_score)}</span>,
									},
									{
										key: "Fix 每回合得分",
										value: (
											<span className="tnum">{formatScore(overview.fix_round_score)}</span>
										),
									},
									{
										key: "开始时间",
										value: overview.started_at ? (
											<span className="tnum">
												{formatDateTime(overview.started_at, { seconds: true })}
											</span>
										) : (
											<span className="text-muted-foreground">未开始</span>
										),
									},
									{
										key: "Fix 开始时间",
										value: overview.fix_started_at ? (
											<span className="tnum">
												{formatDateTime(overview.fix_started_at, { seconds: true })}
											</span>
										) : (
											<span className="text-muted-foreground">未开始</span>
										),
									},
									{
										key: "结束时间",
										value: overview.finished_at ? (
											<span className="tnum">
												{formatDateTime(overview.finished_at, { seconds: true })}
											</span>
										) : (
											<span className="text-muted-foreground">未结束</span>
										),
									},
								]}
							/>
						</SectionCard>

						<div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.6fr)_minmax(0,1.2fr)]">
							<div className="space-y-5">
								<SectionCard
									title="我的靶机"
									description="选择要操作的 GameBox；动作区与实例卡都跟随此选择。"
								>
									<AwdpGameboxList
										gameboxes={gameboxes}
										selectedId={selected?.id ?? null}
										onSelect={setSelectedId}
									/>
								</SectionCard>

								<AwdpInstancePanel
									gamebox={selected}
									instance={instance}
									isPending={
										activePhase && Boolean(selectedGameboxId) && instanceQuery.isPending
									}
									isError={activePhase && instanceQuery.isError}
									error={instanceQuery.error}
									busy={busy}
									phase={phase}
									onStart={() => startMutation.mutate()}
									onStop={() => void askStop()}
									onReset={() => void askReset()}
									onRefresh={() => void instanceQuery.refetch()}
								/>
							</div>

							<AwdpActionPanel
								eventId={eventId}
								gamebox={selected}
								instance={instance}
								phase={phase}
								onChanged={refreshAll}
							/>

							<AwdpRoundsEvaluationsPanel eventId={eventId} enabled={hasRun} />
						</div>

						<SectionCard
							title="积分与趋势"
							description="明细矩阵按题目 × 回合展开；趋势为平台缓存 30s 的数据。"
						>
							<Tabs defaultValue="scoreboard">
								<TabsList>
									<TabsTrigger value="scoreboard">积分榜矩阵</TabsTrigger>
									<TabsTrigger value="scores">我的积分</TabsTrigger>
									<TabsTrigger value="trend">趋势</TabsTrigger>
								</TabsList>
								<TabsContent value="scoreboard" className="pt-3">
									<AwdpScoreboardPanel eventId={eventId} />
								</TabsContent>
								<TabsContent value="scores" className="pt-3">
									<AwdpMyScoresPanel eventId={eventId} />
								</TabsContent>
								<TabsContent value="trend" className="pt-3">
									<AwdpTrendPanel eventId={eventId} />
								</TabsContent>
							</Tabs>
						</SectionCard>

					<div className="flex flex-wrap gap-2">
						<Button variant="outline" size="sm" asChild>
							<Link to={`/events/${eventId}`}>返回赛事工作区</Link>
						</Button>
					</div>
				</div>
			) : null}
		</PageBody>
	);
}
