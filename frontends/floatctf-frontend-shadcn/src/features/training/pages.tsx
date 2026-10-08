/**
 * AWDP Training Ground（练习场）：
 * - `/training` 目录 + 开始训练（`gameboxCatalog` / `startTraining`）
 * - `/training/runs/:runId` run 工作台（生命周期 / 实例 / Break·Fix / 轮次评测 / 积分 / Writeup）
 *
 * 数据全部来自 `client.awdp.runs.*`；实时用 `useAwdpRunStream`。
 */

import { useCallback, useMemo, useState, type ReactNode } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowRight, Dumbbell, Play, RefreshCw, Square, StopCircle, Trophy } from "lucide-react";

import type { AwdpInstance, AwdpRunDto, GameBoxCatalogDto } from "@floatctf/sdk";

import { call, callList, callMaybe, callVoid } from "~/api/call";
import { isForbidden, isNotFound } from "~/api/errors";
import { useBindings, useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { RealtimePill, TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable } from "~/components/app/data-table";
import {
	KeyValueList,
	PageBody,
	PageHeader,
	PaginationBar,
	SectionCard,
	StatCard,
} from "~/components/app/page";
import {
	EmptyBlock,
	ErrorBlock,
	LoadingBlock,
	PermissionDeniedBlock,
	QueryState,
	TableSkeleton,
} from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "~/components/ui/tabs";
import { formatBytes, formatDateTime, formatDuration, formatInt, formatScore } from "~/lib/format";
import { useDebouncedValue, useDocumentTitle } from "~/lib/hooks";

import {
	RunPhaseSteps,
	RunRoundsEvaluationsPanel,
	RunScoresPanel,
	RunWriteupPanel,
	TrainingActionPanel,
	TrainingInstancePanel,
} from "./components";
import {
	RUN_END_CONSEQUENCES,
	RUN_INSTANCE_RESET_CONSEQUENCES,
	RUN_RESET_CONSEQUENCES,
	RUN_RESTART_CONSEQUENCES,
	RUN_SET_PHASE_BREAK_CONSEQUENCES,
	RUN_SET_PHASE_FIX_CONSEQUENCES,
	RUN_STOP_CONSEQUENCES,
	endRunGate,
	restartTrainingGate,
	runPhaseLabel,
	runPhaseTone,
	setPhaseGate,
	startRunGate,
} from "./domain";

// ────────────────────────────────────────────────────────────────────────────
// 目录
// ────────────────────────────────────────────────────────────────────────────

export function TrainingCatalogPage(): ReactNode {
	useDocumentTitle("AWDP 训练场 · FloatCTF");
	const client = useClient();
	const queryClient = useQueryClient();
	const navigate = useNavigate();
	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(12);
	const [search, setSearch] = useState("");
	const debounced = useDebouncedValue(search, 300);
	const filter = debounced.trim() ? `name:${debounced.trim()}` : undefined;
	const params = useMemo(
		() => ({ page, limit: pageSize, ...(filter ? { filter } : {}) }),
		[page, pageSize, filter],
	);

	const query = useQuery({
		queryKey: qk.runs.catalog(params),
		queryFn: () => callList<GameBoxCatalogDto>(client.awdp.runs.gameboxCatalog(params)),
		// 目录里的 active_training / solved 会随 run 生命周期变化：回到本页必须重新取。
		staleTime: 0,
	});

	const startMutation = useMutation({
		mutationFn: (gameboxId: string) =>
			call<AwdpRunDto>(client.awdp.runs.startTraining(gameboxId), "开始训练"),
		onSuccess: (run) => {
			toast.success("训练已就绪", "进入 run 工作台开始 Break 阶段。");
			void queryClient.invalidateQueries({ queryKey: qk.runs.catalog(params) });
			void navigate(`/training/runs/${run.run_id}`);
		},
		onError: (error) => toast.apiError("开始训练失败", error),
	});

	const result = query.data;

	return (
		<PageBody>
			<PageHeader
				title="AWDP 训练场"
				description="选择一个 AWDP 靶机开始练习：Break 破题 → Fix 修复 → 回合判定。"
				actions={
					<Button variant="outline" asChild>
						<Link to="/events">
							<ArrowRight /> 去赛事
						</Link>
					</Button>
				}
			/>

			<SectionCard
				title="练习靶机目录"
				description="目录只暴露安全展示字段（不含 exploit / 源码 / 凭据）。"
				actions={
					<div className="flex items-center gap-2">
						<Input
							value={search}
							placeholder="按名称筛选…"
							aria-label="按名称筛选靶机"
							className="w-48"
							onChange={(event) => {
								setSearch(event.target.value);
								setPage(1);
							}}
						/>
						<Button
							variant="ghost"
							size="icon-sm"
							aria-label="刷新目录"
							onClick={() => void query.refetch()}
						>
							<RefreshCw />
						</Button>
					</div>
				}
				footer={
					result ? (
						<PaginationBar
							page={page}
							pageSize={pageSize}
							total={result.meta.total ?? result.items.length}
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
					errorTitle="加载练习目录失败"
					isEmpty={(data) => data.items.length === 0}
					empty={
						<EmptyBlock
							title={filter ? "没有匹配的靶机" : "暂无可练习靶机"}
							description={
								filter
									? "换个关键词试试，或清空筛选。"
									: "目录里还没有 AWDP-capable 且构建完成的 GameBox。"
							}
							variant={filter ? "filtered" : "empty"}
							action={
								filter ? (
									<Button size="sm" variant="outline" onClick={() => setSearch("")}>
										清空筛选
									</Button>
								) : undefined
							}
						/>
					}
				>
					{(data) => (
						<DataTable
							data={data.items}
							getRowId={(row) => row.id}
							columns={[
								{
									id: "name",
									header: "靶机",
									sortValue: (row) => row.name,
									cell: (row) => (
										<div className="min-w-0">
											<p className="truncate text-sm font-medium">{row.name}</p>
											<p className="line-clamp-1 text-xs text-muted-foreground">
												{row.description || "无描述"}
											</p>
										</div>
									),
								},
								{
									id: "category",
									header: "分类",
									sortValue: (row) => row.category,
									cell: (row) => <span className="text-sm">{row.category || "—"}</span>,
								},
								{
									id: "author",
									header: "作者",
									hideBelow: "md",
									sortValue: (row) => row.author ?? "",
									cell: (row) => (
										<span className="text-sm text-muted-foreground">
											{row.author || "—"}
										</span>
									),
								},
								{
									id: "resources",
									header: "推荐资源",
									hideBelow: "lg",
									cell: (row) => (
										<span className="tnum font-mono text-xs text-muted-foreground">
											{row.recommended_cpu_millis} mCPU · {formatBytes(row.recommended_memory_bytes)} ·{" "}
											{row.recommended_pids_limit} pids
										</span>
									),
								},
								{
									id: "state",
									header: "训练状态",
									cell: (row) => (
										<span className="flex flex-wrap items-center gap-1">
											{row.solved ? <TonePill tone="success">已训练</TonePill> : null}
											{row.active_training ? (
												<TonePill tone="info">
													进行中 · {runPhaseLabel(row.active_training.phase)} ·{" "}
													{formatScore(row.active_training.score)}
												</TonePill>
											) : null}
											{!row.solved && !row.active_training ? (
												<TonePill tone="muted">未开始</TonePill>
											) : null}
										</span>
									),
								},
								{
									id: "updated",
									header: "更新时间",
									hideBelow: "md",
									sortValue: (row) => row.updated_at ?? "",
									cell: (row) => (
										<span className="tnum text-xs text-muted-foreground">
											{row.updated_at
												? formatDateTime(row.updated_at, { seconds: true })
												: "—"}
										</span>
									),
								},
							]}
							rowActions={(row) =>
								row.active_training ? (
									<Button size="sm" asChild>
										<Link to={`/training/runs/${row.active_training.run_id}`}>
											<Play /> 继续训练
										</Link>
									</Button>
								) : (
									<Button
										size="sm"
										disabled={startMutation.isPending}
										onClick={() => startMutation.mutate(row.id)}
									>
										<Dumbbell /> 开始训练
									</Button>
								)
							}
							mobileCard={(row) => (
								<div className="space-y-2 rounded-md border p-3">
									<div className="flex items-center justify-between gap-2">
										<span className="truncate text-sm font-medium">{row.name}</span>
										<TonePill tone={row.solved ? "success" : "muted"}>
											{row.active_training
												? `进行中 · ${runPhaseLabel(row.active_training.phase)}`
												: row.solved
													? "已训练"
													: "未开始"}
										</TonePill>
									</div>
									<p className="line-clamp-2 text-xs text-muted-foreground">
										{row.description || "无描述"}
									</p>
									<div className="flex justify-end">
										{row.active_training ? (
											<Button size="sm" asChild>
												<Link to={`/training/runs/${row.active_training.run_id}`}>
													继续训练
												</Link>
											</Button>
										) : (
											<Button
												size="sm"
												disabled={startMutation.isPending}
												onClick={() => startMutation.mutate(row.id)}
											>
												开始训练
											</Button>
										)}
									</div>
								</div>
							)}
						/>
					)}
				</QueryState>
			</SectionCard>
		</PageBody>
	);
}

// ────────────────────────────────────────────────────────────────────────────
// Run 工作台
// ────────────────────────────────────────────────────────────────────────────

export function TrainingRunPage(): ReactNode {
	const { runId = "" } = useParams<{ runId: string }>();
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const navigate = useNavigate();
	const { useAwdpRunStream } = useBindings();
	const stream = useAwdpRunStream({ runId, enabled: Boolean(runId) });

	const runQuery = useQuery({
		queryKey: qk.runs.run(runId),
		queryFn: () => call<AwdpRunDto>(client.awdp.runs.getRun(runId), "训练 Run"),
		enabled: Boolean(runId),
		retry: false,
	});

	const run = runQuery.data;
	useDocumentTitle(run ? `${run.gamebox_name} · AWDP 训练 · FloatCTF` : "AWDP 训练 · FloatCTF");

	const gameboxId = run?.gamebox_id ?? "";

	// 实例详情：key 以 qk.runs.instance(runId) 为前缀，保持 qk 失效语义。
	const instanceQuery = useQuery({
		queryKey: [...qk.runs.instance(runId), gameboxId],
		queryFn: () =>
			callMaybe<AwdpInstance>(client.awdp.runs.getInstance(runId, gameboxId), "实例详情"),
		enabled: Boolean(runId) && Boolean(gameboxId),
		retry: false,
	});

	// 两个来源：run DTO 的实例快照（SSE 会失效 `awdp-run`）与 getInstance（动作后 / 手动刷新）。
	// 取 dataUpdatedAt 更新的一方，避免 SSE 已刷新的 run 快照被缓存的 getInstance 覆盖。
	const runInstance = run?.instances[0] ?? null;
	const instanceFromRun: AwdpInstance | null = runInstance
		? {
				instance_id: runInstance.instance_id,
				runtime_state: runInstance.runtime_state,
				runtime_generation: runInstance.runtime_generation,
				reset_count: runInstance.reset_count,
				endpoints: runInstance.endpoints,
			}
		: null;
	const instance: AwdpInstance | null =
		instanceQuery.dataUpdatedAt >= runQuery.dataUpdatedAt
			? (instanceQuery.data ?? null)
			: instanceFromRun;

	const refreshAll = useCallback(() => {
		for (const key of [
			qk.runs.run(runId),
			qk.runs.instance(runId),
			qk.runs.rounds(runId),
			qk.runs.evaluations(runId),
			qk.runs.scores(runId),
			qk.runs.writeup(runId),
		]) {
			void queryClient.invalidateQueries({ queryKey: key });
		}
	}, [queryClient, runId]);

	const startMutation = useMutation({
		mutationFn: () => call<AwdpRunDto>(client.awdp.runs.startRun(runId), "开始训练"),
		onSuccess: () => {
			toast.success("训练已开始（实例已启动）");
			refreshAll();
		},
		onError: (error) => toast.apiError("开始训练失败", error),
	});

	const stopMutation = useMutation({
		mutationFn: () => callVoid(client.awdp.runs.stopRun(runId), "停止实例"),
		onSuccess: () => {
			toast.success("实例已停止");
			refreshAll();
		},
		onError: (error) => toast.apiError("停止实例失败", error),
	});

	const resetMutation = useMutation({
		mutationFn: () => call<AwdpRunDto>(client.awdp.runs.resetRun(runId), "重置 run"),
		onSuccess: () => {
			toast.success("run 内实例已重置为原始状态");
			refreshAll();
		},
		onError: (error) => toast.apiError("重置 run 失败", error),
	});

	const endMutation = useMutation({
		mutationFn: () => call<AwdpRunDto>(client.awdp.runs.endRun(runId), "结束训练"),
		onSuccess: () => {
			toast.success("训练已结束", "得分与历史保留；可「再次训练」新建 run。");
			refreshAll();
		},
		onError: (error) => toast.apiError("结束训练失败", error),
	});

	const restartMutation = useMutation({
		mutationFn: () => call<AwdpRunDto>(client.awdp.runs.restartTraining(runId), "再次训练"),
		onSuccess: (next) => {
			toast.success("已创建新的训练 run");
			void navigate(`/training/runs/${next.run_id}`);
		},
		onError: (error) => toast.apiError("再次训练失败", error),
	});

	const phaseMutation = useMutation({
		mutationFn: (target: "break" | "fix") =>
			call<AwdpRunDto>(client.awdp.runs.setPhase(runId, target), "切换阶段"),
		onSuccess: (next, target) => {
			toast.success(target === "fix" ? "已进入 Fix 阶段" : "已回到 Break 阶段");
			// 切换阶段可能新建/回卷 run 数据，直接以返回值刷新缓存。
			queryClient.setQueryData(qk.runs.run(runId), next);
			refreshAll();
		},
		onError: (error) => toast.apiError("切换阶段失败", error),
	});

	const instanceMutation = useMutation({
		mutationFn: async (action: "start" | "stop" | "reset") => {
			if (action === "start") {
				return await call<AwdpInstance>(
					client.awdp.runs.startInstance(runId, gameboxId),
					"启动实例",
				);
			}
			if (action === "stop") {
				await callVoid(client.awdp.runs.stopInstance(runId, gameboxId), "停止实例");
				return null;
			}
			return await call<AwdpInstance>(
				client.awdp.runs.resetInstance(runId, gameboxId),
				"重置实例",
			);
		},
		onSuccess: (_result, action) => {
			toast.success(
				action === "start" ? "实例已启动" : action === "stop" ? "实例已停止" : "实例已重置",
			);
			refreshAll();
		},
		onError: (error) => toast.apiError("实例操作失败", error),
	});

	if (!runId) {
		return (
			<PageBody>
				<EmptyBlock title="缺少 run 标识" description="请从训练场目录进入一个练习 run。" />
			</PageBody>
		);
	}

	return (
		<PageBody>
			<PageHeader
				title={run ? run.gamebox_name : "AWDP 训练 run"}
				description={run?.gamebox_description || "练习 Run 工作台：Break / Fix / 回合判定 / 积分 / Writeup。"}
				breadcrumbs={
					<nav className="flex items-center gap-1.5">
						<Link to="/training" className="hover:underline">
							训练场
						</Link>
						<span>/</span>
						<span className="text-foreground">run {runId.slice(0, 8)}</span>
					</nav>
				}
				badge={run ? <TonePill tone={runPhaseTone(run.phase)}>{runPhaseLabel(run.phase)}</TonePill> : null}
				actions={
					<RealtimePill
						state={stream.connectionState}
						onRefresh={() => {
							stream.invalidateRun();
							refreshAll();
						}}
					/>
				}
			/>

			{runQuery.isPending ? (
				<LoadingBlock label="加载训练 run…" />
			) : runQuery.isError ? (
				isForbidden(runQuery.error) ? (
					<PermissionDeniedBlock description="该训练 run 不属于当前账号，无法查看。" />
				) : isNotFound(runQuery.error) ? (
					<EmptyBlock
						title="未找到该训练 run"
						description="它可能已被删除，或者链接已失效。"
						action={
							<Button variant="outline" size="sm" asChild>
								<Link to="/training">返回训练场</Link>
							</Button>
						}
					/>
				) : (
					<ErrorBlock
						error={runQuery.error}
						title="加载训练 run 失败"
						onRetry={() => void runQuery.refetch()}
					/>
				)
			) : run ? (
				(() => {
					const startGate = startRunGate(run.phase);
					const endGate = endRunGate(run.phase);
					const restartGate = restartTrainingGate(run.phase);
					const toFixGate = setPhaseGate(run.phase, "fix");
					const toBreakGate = setPhaseGate(run.phase, "break");
					const lifecycleBusy =
						startMutation.isPending ||
						stopMutation.isPending ||
						resetMutation.isPending ||
						endMutation.isPending ||
						restartMutation.isPending ||
						phaseMutation.isPending;
					const instanceBusy = instanceMutation.isPending;

					return (
						<div className="space-y-5">
							<SectionCard
								title="训练阶段与生命周期"
								description="练习 run 可以手动控阶段；阶段推进与回合物化由后端完成。"
								actions={
									run.next_action_at ? (
										<span className="tnum text-xs text-muted-foreground">
											下一动作 {formatDateTime(run.next_action_at, { seconds: true })}
										</span>
									) : null
								}
							>
								<RunPhaseSteps
									phase={run.phase}
									currentRound={run.current_round}
									totalRounds={run.total_rounds}
									nextActionAt={run.next_action_at}
								/>

								<div className="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
									<StatCard
										label="我的分数"
										value={formatScore(run.my_score)}
										hint="Break + Fix 累计"
										icon={<Trophy className="size-5" />}
										tone="success"
									/>
									<StatCard
										label="当前回合"
										value={`${formatInt(run.current_round)} / ${formatInt(run.total_rounds)}`}
										hint="Fix 回合"
									/>
									<StatCard
										label="每回合失败扣分"
										value={formatScore(run.fix_round_penalty)}
										hint="练习模式 History 展示用"
									/>
									<StatCard
										label="Break / Fix 时长"
										value={`${formatDuration(run.break_duration_secs)} / ${formatDuration(run.fix_duration_secs)}`}
										hint={`回合间隔 ${formatDuration(run.fix_round_interval_secs)}`}
									/>
								</div>

								<KeyValueList
									className="mt-4"
									columns={3}
									items={[
										{
											key: "Break 单题得分",
											value: <span className="tnum">{formatScore(run.break_score)}</span>,
										},
										{
											key: "Fix 每回合得分",
											value: (
												<span className="tnum">{formatScore(run.fix_round_score)}</span>
											),
										},
										{
											key: "开始时间",
											value: run.started_at ? (
												<span className="tnum">
													{formatDateTime(run.started_at, { seconds: true })}
												</span>
											) : (
												<span className="text-muted-foreground">未开始</span>
											),
										},
										{
											key: "Fix 开始时间",
											value: run.fix_started_at ? (
												<span className="tnum">
													{formatDateTime(run.fix_started_at, { seconds: true })}
												</span>
											) : (
												<span className="text-muted-foreground">未开始</span>
											),
										},
										{
											key: "结束时间",
											value: run.finished_at ? (
												<span className="tnum">
													{formatDateTime(run.finished_at, { seconds: true })}
												</span>
											) : (
												<span className="text-muted-foreground">未结束</span>
											),
										},
										{
											key: "源码目录",
											value: run.source_code_dir ? (
												<MonoTextValue value={run.source_code_dir} />
											) : (
												<span className="text-muted-foreground">
													Fix 阶段才下发
												</span>
											),
										},
									]}
								/>

								<div className="mt-4 flex flex-wrap items-center gap-2">
									<Button
										size="sm"
										disabled={!startGate.allowed || lifecycleBusy}
										title={startGate.allowed ? "启动实例并进入会话" : startGate.reason}
										onClick={() => startMutation.mutate()}
									>
										<Play /> 开始训练
									</Button>
									<Button
										variant="outline"
										size="sm"
										disabled={lifecycleBusy}
										title="停止 run 内全部实例（保留 run 与端点）"
										onClick={async () => {
											const ok = await confirm({
												title: "停止 run 内全部实例？",
												description: "停止后端点不再可用，官方回合判定无法执行。",
												consequences: RUN_STOP_CONSEQUENCES,
												tone: "danger",
												confirmText: "停止",
											});
											if (ok) stopMutation.mutate();
										}}
									>
										<Square /> 停止实例
									</Button>
									<Button
										variant="outline"
										size="sm"
										disabled={lifecycleBusy}
										title="重置 run 内全部实例为原始状态（破坏性）"
										onClick={async () => {
											const ok = await confirm({
												title: "重置 run 内全部实例？",
												description: "实例会被销毁并按原始镜像重建。",
												consequences: RUN_RESET_CONSEQUENCES,
												tone: "danger",
												confirmText: "重置",
											});
											if (ok) resetMutation.mutate();
										}}
									>
										<RefreshCw /> 重置 run
									</Button>
									<Button
										variant="outline"
										size="sm"
										disabled={!endGate.allowed || lifecycleBusy}
										title={endGate.allowed ? "结束本次训练" : endGate.reason}
										onClick={async () => {
											const ok = await confirm({
												title: "结束本次训练？",
												description: "训练会话结束，实例被停止。",
												consequences: RUN_END_CONSEQUENCES,
												tone: "danger",
												confirmText: "结束训练",
											});
											if (ok) endMutation.mutate();
										}}
									>
										<StopCircle /> 结束训练
									</Button>
									<Button
										variant="outline"
										size="sm"
										disabled={!restartGate.allowed || lifecycleBusy}
										title={restartGate.allowed ? "基于同一 GameBox 新建 run" : restartGate.reason}
										onClick={async () => {
											const ok = await confirm({
												title: "再次训练（新建 run）？",
												description: "会创建一个从 0 计分的新 run，并跳转到它的页面。",
												consequences: RUN_RESTART_CONSEQUENCES,
												confirmText: "再次训练",
											});
											if (ok) restartMutation.mutate();
										}}
									>
										<Dumbbell /> 再次训练
									</Button>
									{endGate.allowed ? (
										<Button
											variant="ghost"
											size="sm"
											asChild
											title="不结束 run，只查看历史"
										>
											<Link to="/training">返回目录</Link>
										</Button>
									) : null}
								</div>

								{!startGate.allowed || !endGate.allowed || !restartGate.allowed ? (
									<ul className="mt-2 space-y-1 text-xs text-muted-foreground">
										{!startGate.allowed ? <li>开始训练：{startGate.reason}</li> : null}
										{!endGate.allowed ? <li>结束训练：{endGate.reason}</li> : null}
										{!restartGate.allowed ? <li>再次训练：{restartGate.reason}</li> : null}
									</ul>
								) : null}

								<div className="mt-4 flex flex-wrap items-center gap-2 border-t pt-3">
									<span className="text-xs text-muted-foreground">手动切换阶段：</span>
									<Button
										variant="outline"
										size="sm"
										disabled={!toFixGate.allowed || lifecycleBusy}
										title={toFixGate.allowed ? "进入 Fix 阶段（重置实例）" : toFixGate.reason}
										onClick={async () => {
											const ok = await confirm({
												title: "进入 Fix 阶段？",
												description: "实例会被重置为原始状态，开始物化回合时间线。",
												consequences: RUN_SET_PHASE_FIX_CONSEQUENCES,
												confirmText: "进入 Fix",
											});
											if (ok) phaseMutation.mutate("fix");
										}}
									>
										进入 Fix
									</Button>
									<Button
										variant="outline"
										size="sm"
										disabled={!toBreakGate.allowed || lifecycleBusy}
										title={toBreakGate.allowed ? "回到 Break 阶段（撤销 Fix 会话）" : toBreakGate.reason}
										onClick={async () => {
											const ok = await confirm({
												title: "回到 Break 阶段？",
												description:
													"这会撤销整个 Fix 会话：回合、评估与 Fix 计分全部清零。",
												consequences: RUN_SET_PHASE_BREAK_CONSEQUENCES,
												tone: "danger",
												confirmText: "回到 Break",
											});
											if (ok) phaseMutation.mutate("break");
										}}
									>
										回到 Break
									</Button>
									{!toFixGate.allowed ? (
										<span className="text-xs text-muted-foreground">{toFixGate.reason}</span>
									) : null}
								</div>
							</SectionCard>

							<div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.6fr)_minmax(0,1.2fr)]">
								<TrainingInstancePanel
									gameboxName={run.gamebox_name}
									broken={runInstance?.broken ?? false}
									instance={instance}
									isPending={Boolean(gameboxId) && instanceQuery.isPending}
									isError={instanceQuery.isError}
									error={instanceQuery.error}
									judgeEndpoint={run.judge_endpoint}
									busy={instanceBusy}
									phase={run.phase}
									onStart={() => instanceMutation.mutate("start")}
									onStop={async () => {
										const ok = await confirm({
											title: "停止实例？",
											description: "停止后端点不再可用，官方回合判定无法执行。",
											consequences: RUN_STOP_CONSEQUENCES,
											tone: "danger",
											confirmText: "停止",
										});
										if (ok) instanceMutation.mutate("stop");
									}}
									onReset={async () => {
										const ok = await confirm({
											title: "重置实例？",
											description: "实例会被销毁并按原始镜像重建（pristine）。",
											consequences: RUN_INSTANCE_RESET_CONSEQUENCES,
											tone: "danger",
											confirmText: "重置",
										});
										if (ok) instanceMutation.mutate("reset");
									}}
									onRefresh={() => void instanceQuery.refetch()}
								/>

								<TrainingActionPanel
									runId={runId}
									gameboxId={gameboxId}
									phase={run.phase}
									instance={instance}
									onChanged={refreshAll}
								/>

								<RunRoundsEvaluationsPanel runId={runId} />
							</div>

							<SectionCard
								title="积分明细与 Writeup"
								description="ALL Check 通过会结束 run，并在积分明细里出现扫尾计分记录。"
							>
								<Tabs defaultValue="scores">
									<TabsList>
										<TabsTrigger value="scores">积分明细</TabsTrigger>
										<TabsTrigger value="writeup">Writeup</TabsTrigger>
									</TabsList>
									<TabsContent value="scores" className="pt-3">
										<RunScoresPanel runId={runId} />
									</TabsContent>
									<TabsContent value="writeup" className="pt-3">
										<RunWriteupPanel runId={runId} />
									</TabsContent>
								</Tabs>
							</SectionCard>

						</div>
					);
				})()
			) : null}
		</PageBody>
	);
}

/** 源码目录用等宽展示（避免在 KeyValueList 里塞长文本时换行错乱）。 */
function MonoTextValue({ value }: { value: string }): ReactNode {
	return <code className="tnum font-mono text-xs break-all">{value}</code>;
}
