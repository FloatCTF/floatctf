/**
 * AWDP 练习 Run 工作台。
 *
 * ## 与比赛工作台的差别（这不是抄一份）
 *
 * 练习有比赛没有的能力：**自由切换阶段**（`setPhase`）、**全量校验**（`allCheck`）、
 * **重开**（`restartTraining`/`resetRun`）、以及**题解记录**（`getWriteup`/`saveWriteup`）。
 * 因此这一页在这些能力上比比赛工作台更丰富 —— 但**计分语义更轻**：
 * 练习分不进任何排行榜，所以这里不做"罚分/回合收益"那套沉重提示。
 *
 * ## 实时
 *
 * 用 `useAwdpRunStream`（SSE 路径 `/service/awdp/runs/<runId>/stream`）。
 */
import { useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router";
import {
	Button,
	Card,
	Divider,
	Input,
	Notification,
	Tabs,
	Tag,
	Tooltip,
	Upload,
} from "animal-island-ui";
import {
	CheckCircleIcon,
	ClockIcon,
	CodeIcon,
	DownloadIcon,
	FlagIcon,
	GlobeIcon,
	PlayIcon,
	RefreshIcon,
	SaveIcon,
	StopIcon,
	WifiIcon,
} from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { EmptyState, ErrorState, LoadingState, SkeletonCards } from "~/components/states";
import { StatCard } from "~/components/resource";
import { formatTime } from "~/lib/event";
import { createBindings } from "~/api/react";

const PHASE_LABEL: Record<string, string> = {
	pending: "未开始",
	break: "Break（攻破）",
	preparing_fix: "准备修复",
	fix: "Fix（修复）",
	ended: "已结束",
};

export function AwdpRun() {
	const { runId = "" } = useParams();
	const runtime = useRuntime();
	const navigate = useNavigate();
	const confirm = useConfirm();

	const bindings = useState(() => createBindings(runtime.client))[0];
	const stream = bindings.useAwdpRunStream({ runId, enabled: Boolean(runId) });

	const run = useUniQuery(
		["awdp-run", runId],
		() => runtime.client.awdp.runs.getRun(runId),
		"加载练习",
		{ enabled: Boolean(runId) },
	);

	const api = runtime.client.awdp.runs;
	const startRun = useUniMutation(() => api.startRun(runId), "开始练习");
	const stopRun = useUniMutation(() => api.stopRun(runId), "停止练习");
	const resetRun = useUniMutation(() => api.resetRun(runId), "重置练习");
	const endRun = useUniMutation(() => api.endRun(runId), "结束练习");
	const restart = useUniMutation(() => api.restartTraining(runId), "重新训练");
	const setPhase = useUniMutation(
		(vars: "break" | "fix") => api.setPhase(runId, vars),
		"切换阶段",
	);
	const startInstance = useUniMutation(() => api.startInstance(runId, gameboxId), "启动实例");
	const stopInstance = useUniMutation(() => api.stopInstance(runId, gameboxId), "停止实例");
	const resetInstance = useUniMutation(() => api.resetInstance(runId, gameboxId), "重置实例");
	const submit = useUniMutation(
		(vars: { flag: string }) => api.submitBreak(runId, gameboxId, vars.flag),
		"提交 flag",
	);
	const upload = useUniMutation(
		(vars: { file: File }) => api.uploadPatch(runId, gameboxId, vars.file),
		"上传补丁",
	);
	const testCheck = useUniMutation(() => api.testCheck(runId, gameboxId), "手工自检");
	const allCheck = useUniMutation(() => api.allCheck(runId, gameboxId), "全量校验");
	const sourceUrl = useUniMutation(() => api.sourceUrl(runId, gameboxId), "获取源码链接");

	const [flag, setFlag] = useState("");
	const gameboxId = run.data?.gamebox_id ?? "";

	if (!runId) {
		return (
			<Card>
				<EmptyState
					title="没有指定练习"
					description="练习工作台必须绑定到一个 run。"
					action={<Button onClick={() => void navigate("/island/awdp/practice")}>回练习目录</Button>}
				/>
			</Card>
		);
	}

	if (run.isLoading) return <LoadingState label="加载练习…" />;
	if (run.error) return <ErrorState error={run.error} onRetry={() => void run.refetch()} />;
	if (!run.data) {
		return (
			<Card>
				<EmptyState
					title="练习不存在"
					description="它可能已被结束或删除。"
					action={<Button onClick={() => void navigate("/island/awdp/practice")}>回练习目录</Button>}
				/>
			</Card>
		);
	}

	const r = run.data;

	return (
		<>
			<PageHeader
				title={`练习：${r.gamebox_name}`}
				description={r.gamebox_description || "AWDP 练习"}
				extra={
					<>
						<Tooltip
							title={stream.connected ? "事件流已连接" : "事件流不可用，正在轮询兜底"}
						>
							<Tag
								size="small"
								variant="soft"
								color={stream.connected ? "app-green" : "app-yellow"}
							>
								<WifiIcon size={12} />
								{stream.connected ? "实时" : "轮询兜底"}
							</Tag>
						</Tooltip>
						<Tag size="small" variant="solid" color="app-teal">
							{PHASE_LABEL[r.phase] ?? r.phase}
						</Tag>
						<Button
							size="small"
							icon={<RefreshIcon size={16} />}
							onClick={() => void run.refetch()}
						>
							刷新
						</Button>
						<Button
							size="small"
							icon={<PlayIcon size={16} />}
							loading={setPhase.isPending}
							onClick={async () => {
								const next = r.phase === "break" ? "fix" : "break";
								await setPhase.mutateAsync(next);
								Notification.success({ message: `已切换到 ${PHASE_LABEL[next]}` });
								void run.refetch();
							}}
						>
							切到 {r.phase === "break" ? "Fix" : "Break"}
						</Button>
					</>
				}
			/>

			<div className="ai-grid ai-grid-wide">
				<StatCard label="练习分" value={r.my_score} icon={<FlagIcon size={18} />} />
				<StatCard
					label="当前回合"
					value={`${r.current_round}/${r.total_rounds}`}
					icon={<ClockIcon size={18} />}
				/>
				<StatCard label="Break 单题分" value={r.break_score} />
				<StatCard label="Fix 每回合分" value={r.fix_round_score} />
				<StatCard
					label="每轮失败罚分"
					value={r.fix_round_penalty}
					hint="练习模式独有"
				/>
				<StatCard
					label="下一个节点"
					value={
						<span style={{ fontSize: 14 }}>
							{r.next_action_at ? formatTime(r.next_action_at) : "—"}
						</span>
					}
				/>
			</div>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<span className="ai-strong" style={{ fontSize: 18 }}>
						练习控制
					</span>
					<span className="ai-muted">
						练习可以自由开始/停止/重置/重开。这些操作**不会**影响任何比赛数据。
					</span>
					<Divider />
					<div className="ai-row" style={{ gap: 8, flexWrap: "wrap" }}>
						<Button
							type="primary"
							icon={<PlayIcon size={14} />}
							loading={startRun.isPending}
							onClick={async () => {
								await startRun.mutateAsync(undefined);
								Notification.success({ message: "已开始" });
								void run.refetch();
							}}
						>
							开始
						</Button>
						<Button
							icon={<StopIcon size={14} />}
							loading={stopRun.isPending}
							onClick={async () => {
								await stopRun.mutateAsync(undefined);
								void run.refetch();
							}}
						>
							停止
						</Button>
						<Button
							danger
							icon={<RefreshIcon size={14} />}
							loading={resetRun.isPending}
							onClick={async () => {
								const ok = await confirm({
									title: "重置练习",
									consequence:
										"练习会回到当前阶段的起点：**实例会被重建，已上传的补丁会丢失**。练习分与历史评测保留（这是练习，不是比赛）。",
									okText: "重置",
									danger: true,
								});
								if (!ok) return;
								await resetRun.mutateAsync(undefined);
								void run.refetch();
							}}
						>
							重置
						</Button>
						<Button
							icon={<RefreshIcon size={14} />}
							loading={restart.isPending}
							onClick={async () => {
								const ok = await confirm({
									title: "重新训练",
									consequence:
										"会**从零开始一个新的练习**（回合、分数、评测全部重置）。旧的练习记录会保留在历史里但不再可操作。",
									okText: "重新训练",
									danger: true,
								});
								if (!ok) return;
								const fresh = await restart.mutateAsync(undefined);
								if (fresh?.run_id) void navigate(`/island/awdp/runs/${fresh.run_id}`);
							}}
						>
							重新训练
						</Button>
						<Button
							danger
							icon={<StopIcon size={14} />}
							loading={endRun.isPending}
							onClick={async () => {
								const ok = await confirm({
									title: "结束练习",
									consequence: "练习会进入已结束状态，之后不能继续提交或修复。",
									okText: "结束",
									danger: true,
								});
								if (!ok) return;
								await endRun.mutateAsync(undefined);
								void run.refetch();
							}}
						>
							结束
						</Button>
						<Button
							icon={<DownloadIcon size={14} />}
							loading={sourceUrl.isPending}
							onClick={async () => {
								const url = await sourceUrl.mutateAsync(undefined);
								if (typeof url === "string" && url) window.open(url, "_blank", "noopener");
								else Notification.info({ message: "这个箱子没有提供源码" });
							}}
						>
							下载源码
						</Button>
					</div>
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<span className="ai-strong" style={{ fontSize: 18 }}>
						实例
					</span>
					<Divider />
					<InstanceSection
						runId={runId}
						gameboxId={gameboxId}
						onStart={() => startInstance.mutateAsync(undefined)}
						onStop={() => stopInstance.mutateAsync(undefined)}
						onReset={() => resetInstance.mutateAsync(undefined)}
						starting={startInstance.isPending}
						stopping={stopInstance.isPending}
						resetting={resetInstance.isPending}
					/>
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<span className="ai-strong" style={{ fontSize: 18 }}>
						破题与修复
					</span>
					<Divider />

					<div className="ai-row" style={{ gap: 8 }}>
						<div style={{ flex: 1, minWidth: 220 }}>
							<Input
								allowClear
								placeholder="flag{...}"
								prefix={<FlagIcon size={16} />}
								value={flag}
								onChange={(e) => setFlag(e.target.value)}
							/>
						</div>
						<Button
							type="primary"
							icon={<FlagIcon size={16} />}
							loading={submit.isPending}
							disabled={!flag.trim()}
							onClick={async () => {
								const res = await submit.mutateAsync({ flag: flag.trim() });
								if (res?.accepted && res?.scored) {
									Notification.success({ message: "攻破成功" });
								} else if (res?.already_broken) {
									Notification.info({ message: "这个练习此前已攻破" });
								} else if (res?.accepted) {
									Notification.info({ message: "flag 正确，但本次未计分" });
								} else {
									Notification.error({ message: "flag 不正确" });
								}
								setFlag("");
								void run.refetch();
							}}
						>
							提交
						</Button>
					</div>

					<div className="ai-row" style={{ gap: 8, flexWrap: "wrap" }}>
						<Upload
							showUploadList={false}
							accept=".patch,.diff,.zip,.tar.gz"
							customRequest={({ file, onSuccess, onError }) => {
								upload
									.mutateAsync({ file })
									.then((res) => {
										if (res?.status === "applied") {
											Notification.success({ message: "补丁已应用" });
										} else {
											Notification.error({
												message: "补丁未通过",
												description: res?.error_message ?? "补丁未能应用。",
											});
										}
										onSuccess(res);
										void run.refetch();
									})
									.catch(onError);
							}}
						>
							<Button icon={<CodeIcon size={14} />}>上传补丁</Button>
						</Upload>
						<Button
							icon={<CheckCircleIcon size={14} />}
							loading={testCheck.isPending}
							onClick={async () => {
								const res = await testCheck.mutateAsync(undefined);
								Notification.info({
									message: `自检：${res?.status ?? "完成"}`,
									description:
										[
											res?.healthcheck_ok === false ? "健康检查未通过" : null,
											res?.judge_ok === false ? "判题未通过" : null,
											res?.exploit_ok === false ? "exploit 仍可打（不计分）" : null,
										]
											.filter(Boolean)
											.join("；") || "各项检查已通过。",
								});
							}}
						>
							手工自检
						</Button>
						<Button
							icon={<CheckCircleIcon size={14} />}
							loading={allCheck.isPending}
							onClick={async () => {
								const ok = await confirm({
									title: "全量校验",
									consequence:
										"会立即对**剩余全部回合**执行评测。练习里这是安全的（分数独立于比赛），但会真实占用判题资源，可能需要一段时间。",
									okText: "开始全量校验",
								});
								if (!ok) return;
								const res = await allCheck.mutateAsync(undefined);
								Notification.info({
									message: `全量校验：${res?.status ?? "完成"}`,
									description: res?.swept
										? `已扫过 ${res.swept_rounds} 个回合，目标回合 ${res.target_round}。`
										: `目标回合 ${res?.target_round ?? "—"}：本次未通过。`,
								});
								void run.refetch();
							}}
						>
							全量校验
						</Button>
					</div>
				</div>
			</Card>

			<Card>
				<Tabs
					aria-label="练习分区"
					defaultActiveKey="rounds"
					items={[
						{ key: "rounds", label: "回合", children: <RunRounds runId={runId} /> },
						{ key: "evaluations", label: "评测", children: <RunEvaluations runId={runId} /> },
						{ key: "scores", label: "积分流水", children: <RunScores runId={runId} /> },
						{ key: "writeup", label: "题解", children: <RunWriteup runId={runId} /> },
					]}
				/>
			</Card>
		</>
	);
}

/** 实例区：查询 + 生命周期。 */
function InstanceSection({
	runId,
	gameboxId,
	onStart,
	onStop,
	onReset,
	starting,
	stopping,
	resetting,
}: {
	runId: string;
	gameboxId: string;
	onStart: () => Promise<unknown>;
	onStop: () => Promise<unknown>;
	onReset: () => Promise<unknown>;
	starting: boolean;
	stopping: boolean;
	resetting: boolean;
}) {
	const runtime = useRuntime();
	const confirm = useConfirm();
	const inst = useUniQuery(
		["awdp-run-instance", runId, gameboxId],
		() => runtime.client.awdp.runs.getInstance(runId, gameboxId),
		"加载实例",
		{ enabled: Boolean(runId) && Boolean(gameboxId) },
	);

	if (!gameboxId) return <SkeletonCards count={1} />;
	if (inst.isLoading) return <SkeletonCards count={1} />;
	if (inst.error) return <ErrorState error={inst.error} onRetry={() => void inst.refetch()} />;

	if (!inst.data) {
		return (
			<div className="ai-row" style={{ gap: 8 }}>
				<Button
					type="primary"
					icon={<PlayIcon size={14} />}
					loading={starting}
					onClick={async () => {
						await onStart();
						Notification.success({ message: "实例已启动" });
						void inst.refetch();
					}}
				>
					启动实例
				</Button>
				<span className="ai-muted">练习环境还没启动。</span>
			</div>
		);
	}

	const i = inst.data;
	return (
		<div className="ai-col" style={{ gap: 8 }}>
			<div className="ai-row" style={{ gap: 8 }}>
				<Tag size="small" variant="soft" color={/running|up/i.test(i.runtime_state) ? "app-green" : "brown"}>
					{i.runtime_state}
				</Tag>
				<span className="ai-muted" style={{ fontSize: 12 }}>
					第 {i.runtime_generation} 代 ｜ 已重置 {i.reset_count} 次
				</span>
			</div>
			{i.endpoints.length > 0 ? (
				<div className="ai-col" style={{ gap: 4 }}>
					{i.endpoints.map((ep) => (
						<div key={`${ep.protocol}-${ep.public_port}`} className="ai-row" style={{ gap: 8 }}>
							<GlobeIcon size={14} />
							<span className="ai-mono">
								{ep.protocol}://{ep.public_host}:{ep.public_port}
							</span>
						</div>
					))}
				</div>
			) : null}
			<div className="ai-row" style={{ gap: 8 }}>
				<Button
					size="small"
					icon={<StopIcon size={14} />}
					loading={stopping}
					onClick={async () => {
						await onStop();
						void inst.refetch();
					}}
				>
					停止
				</Button>
				<Button
					size="small"
					danger
					icon={<RefreshIcon size={14} />}
					loading={resetting}
					onClick={async () => {
						const ok = await confirm({
							title: "重置练习实例",
							consequence:
								"实例会被重建为初始状态，**你在里面做的修改与已上传的补丁都会丢失**（练习分不回滚）。",
							okText: "重置",
							danger: true,
						});
						if (!ok) return;
						await onReset();
						void inst.refetch();
					}}
				>
					重置
				</Button>
			</div>
		</div>
	);
}

function RunRounds({ runId }: { runId: string }) {
	const runtime = useRuntime();
	const q = useUniQuery(
		["awdp-run-rounds", runId],
		() => runtime.client.awdp.runs.rounds(runId),
		"加载回合",
	);
	if (q.isLoading) return <SkeletonCards count={2} />;
	if (q.error) return <ErrorState error={q.error} onRetry={() => void q.refetch()} />;
	if ((q.data ?? []).length === 0) return <EmptyState title="还没有回合" />;
	return (
		<div className="ai-col" style={{ gap: 8 }}>
			{(q.data ?? []).map((r) => (
				<div key={r.id} className="ai-row-between">
					<div className="ai-row" style={{ gap: 8 }}>
						<Tag size="small" variant="outlined">
							第 {r.sequence} 回合
						</Tag>
						<Tag size="small" variant="soft" color={r.status === "completed" ? "app-green" : "brown"}>
							{r.status}
						</Tag>
					</div>
					<span className="ai-muted" style={{ fontSize: 12 }}>
						{formatTime(r.starts_at)} 至 {formatTime(r.cutoff_at)}
					</span>
				</div>
			))}
		</div>
	);
}

function RunEvaluations({ runId }: { runId: string }) {
	const runtime = useRuntime();
	const q = useUniQuery(
		["awdp-run-evaluations", runId],
		() => runtime.client.awdp.runs.evaluations(runId),
		"加载评测",
	);
	if (q.isLoading) return <SkeletonCards count={2} />;
	if (q.error) return <ErrorState error={q.error} onRetry={() => void q.refetch()} />;
	if ((q.data ?? []).length === 0) {
		return <EmptyState title="还没有评测" description="上传补丁并跑过回合后会出现结果。" />;
	}
	return (
		<div className="ai-col" style={{ gap: 8 }}>
			{(q.data ?? []).map((e) => (
				<div key={e.id} className="ai-row-between">
					<div className="ai-row" style={{ gap: 8 }}>
						<Tag size="small" variant={e.kind === "official" ? "soft" : "outlined"}>
							{e.kind === "official" ? "官方" : "手工"}
						</Tag>
						<span className="ai-muted">回合 {e.round_sequence ?? "—"}</span>
						<Tag
							size="small"
							variant="soft"
							color={e.status === "completed" ? "app-green" : e.status === "failed" ? "app-red" : "app-yellow"}
						>
							{e.status}
						</Tag>
					</div>
					<span className="ai-muted" style={{ fontSize: 12 }}>
						{e.healthcheck_result ?? "—"} / {e.judge_result ?? "—"}
					</span>
				</div>
			))}
		</div>
	);
}

function RunScores({ runId }: { runId: string }) {
	const runtime = useRuntime();
	const q = useUniQuery(
		["awdp-run-scores", runId],
		() => runtime.client.awdp.runs.scores(runId),
		"加载积分流水",
	);
	if (q.isLoading) return <SkeletonCards count={2} />;
	if (q.error) return <ErrorState error={q.error} onRetry={() => void q.refetch()} />;
	const d = q.data;
	if (!d) return <EmptyState title="没有积分数据" />;
	return (
		<div className="ai-col" style={{ gap: 10 }}>
			<div className="ai-row" style={{ gap: 8 }}>
				<Tag size="small" variant="soft" color="app-teal">
					合计 {d.total} 分
				</Tag>
				<span className="ai-muted" style={{ fontSize: 12 }}>{d.history.length} 条流水</span>
			</div>
			{d.history.length === 0 ? (
				<EmptyState title="还没有积分流水" />
			) : (
				<div className="ai-col" style={{ gap: 6 }}>
					{d.history.map((h) => (
						<div key={h.id} className="ai-row-between">
							<div className="ai-row" style={{ gap: 8 }}>
								<Tag
									size="small"
									variant="soft"
									color={h.score_type === "break" ? "app-red" : "app-green"}
								>
									{h.score_type === "break" ? "破题" : "修复"}
								</Tag>
								<span className="ai-mono ai-muted" style={{ fontSize: 12 }}>
									{h.gamebox_id.slice(0, 8)}
								</span>
							</div>
							<div className="ai-row" style={{ gap: 8 }}>
								<span className="ai-strong">
									{h.delta >= 0 ? "+" : ""}
									{h.delta}
								</span>
								<span className="ai-muted" style={{ fontSize: 12 }}>
									{formatTime(h.created_at)}
								</span>
							</div>
						</div>
					))}
				</div>
			)}
		</div>
	);
}

function RunWriteup({ runId }: { runId: string }) {
	const runtime = useRuntime();
	const [content, setContent] = useState("");
	const [loaded, setLoaded] = useState(false);

	const q = useUniQuery(
		["awdp-run-writeup", runId],
		() => runtime.client.awdp.runs.getWriteup(runId),
		"加载题解",
	);
	const save = useUniMutation(
		(vars: { content: string }) => runtime.client.awdp.runs.saveWriteup(runId, vars.content),
		"保存题解",
	);

	// 只在首次加载后灌入编辑器，避免覆盖用户正在输入的内容。
	useEffect(() => {
		if (!loaded && q.data) {
			setContent(q.data.content ?? "");
			setLoaded(true);
		}
	}, [q.data, loaded]);

	if (q.isLoading) return <SkeletonCards count={1} />;
	if (q.error) return <ErrorState error={q.error} onRetry={() => void q.refetch()} />;

	return (
		<div className="ai-col" style={{ gap: 10 }}>
			<span className="ai-muted">
				练习题解只属于你自己，用于记录思路。{q.data?.updated_at ? `上次保存：${formatTime(q.data.updated_at)}` : "还没有保存过。"}
			</span>
			<Input
				value={content}
				placeholder="记录你的破题与修复思路…"
				onChange={(e) => setContent(e.target.value)}
			/>
			<div className="ai-row" style={{ justifyContent: "flex-end" }}>
				<Button
					type="primary"
					icon={<SaveIcon size={16} />}
					loading={save.isPending}
					disabled={!content.trim()}
					onClick={async () => {
						await save.mutateAsync({ content: content.trim() });
						Notification.success({ message: "题解已保存" });
						void q.refetch();
					}}
				>
					保存
				</Button>
			</div>
		</div>
	);
}
