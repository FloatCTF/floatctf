/**
 * AWDP 选手工作台（比赛模式）。
 *
 * ## 阶段是这一页的主轴
 *
 * AWDP 与 AWD 不同：它有**两个性质完全不同的阶段**，而且玩家在每个阶段做的是**不同的事**：
 *   - `break`（攻破）：找漏洞、提交 flag —— 与 AWD 的攻击类似；
 *   - `preparing_fix` / `fix`（修复）：上传补丁，由判题器在**后续每个回合**持续验证。
 *
 * 因此界面的分栏不是按"数据类型"（列表/详情），而是按**当前阶段该做什么**。
 * 在 break 阶段把补丁上传摆在最前面是错的 —— 那时它还没有意义。
 *
 * ## 补丁是被反复评测的，不是"提交一次就结束"
 *
 * `fix_round_score` × 剩余回合意味着补丁的收益取决于它**能撑住多少个回合**。
 * 因此确认弹窗里写的是这个语义，而不是简单的"上传成功"。
 *
 * ## 实时
 *
 * 用 `useAwdpEventStream`（`@floatctf/react`）。事件流的失效 key 与页面读取的
 * query key 对齐 —— 理由见 `~/api/react.ts`：SSE 连上后轮询会停，key 漂移会让面板静默停更。
 */
import { useState } from "react";
import { useNavigate, useParams } from "react-router";
import {
	Button,
	Card,
	Divider,
	Input,
	Loading,
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
import type { AwdpGameBox, AwdpInstance } from "@floatctf/sdk";

/** 阶段文案：把后端枚举翻译成"我现在该干什么"。 */
const PHASE_META: Record<string, { label: string; hint: string; color: string }> = {
	pending: { label: "未开始", hint: "等待管理员开始比赛。", color: "brown" },
	break: { label: "Break（攻破）", hint: "找漏洞并提交 flag。", color: "app-red" },
	preparing_fix: {
		label: "准备修复",
		hint: "Break 已结束，修复阶段即将开始。可以先把源码拉下来。",
		color: "app-yellow",
	},
	fix: { label: "Fix（修复）", hint: "上传补丁；补丁会在后续每个回合被重新评测。", color: "app-green" },
	ended: { label: "已结束", hint: "比赛已结束，分数定格。", color: "brown" },
};

export function AwdpEvent() {
	const { eventId = "" } = useParams();
	const runtime = useRuntime();
	const navigate = useNavigate();

	const bindings = useState(() => createBindings(runtime.client))[0];
	const stream = bindings.useAwdpEventStream({ eventId, enabled: Boolean(eventId) });

	const overview = useUniQuery(
		["awdp-overview", eventId],
		() => runtime.client.awdp.player.overview(eventId),
		"加载 AWDP 总览",
		{ enabled: Boolean(eventId) },
	);

	if (!eventId) {
		return (
			<Card>
				<EmptyState
					title="没有指定赛事"
					description="AWDP 工作台必须绑定到一场赛事。"
					action={<Button onClick={() => void navigate("/island/events")}>去赛事列表</Button>}
				/>
			</Card>
		);
	}

	if (overview.isLoading) return <LoadingState label="加载 AWDP 工作台…" />;
	if (overview.error) {
		return <ErrorState error={overview.error} onRetry={() => void overview.refetch()} />;
	}
	if (!overview.data) {
		return (
			<ErrorState
				error={new Error("后端没有返回 AWDP 总览")}
				onRetry={() => void overview.refetch()}
			/>
		);
	}

	const o = overview.data;
	const meta = PHASE_META[o.phase] ?? {
		label: o.phase,
		hint: "",
		color: "brown",
	};

	return (
		<>
			<PageHeader
				title="AWDP 工作台"
				description={meta.hint}
				extra={
					<>
						<Tooltip
							title={
								stream.connected
									? "事件流已连接，阶段与分数会自动刷新"
									: "事件流不可用，正在用轮询兜底"
							}
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
						<Tag size="small" variant="solid" color={meta.color as "app-red"}>
							{meta.label}
						</Tag>
						<Button
							size="small"
							icon={<RefreshIcon size={16} />}
							onClick={() => void overview.refetch()}
						>
							刷新
						</Button>
					</>
				}
			/>

			<div className="ai-grid ai-grid-wide">
				<StatCard label="我的总分" value={o.my_score} icon={<FlagIcon size={18} />} />
				<StatCard
					label="当前回合"
					value={`${o.current_round}/${o.total_rounds}`}
					icon={<ClockIcon size={18} />}
				/>
				<StatCard label="Break 单题分" value={o.break_score} />
				<StatCard
					label="Fix 每回合分"
					value={o.fix_round_score}
					hint="补丁每撑住一个回合的收益"
				/>
				<StatCard
					label="下一个节点"
					value={
						<span style={{ fontSize: 14 }}>
							{o.next_action_at ? formatTime(o.next_action_at) : "—"}
						</span>
					}
				/>
				<StatCard
					label="已攻破"
					value={o.gameboxes.filter((g) => g.broken).length}
					hint={`共 ${o.gameboxes.length} 题`}
				/>
			</div>

			<Card>
				<div className="ai-row" style={{ gap: 14, flexWrap: "wrap" }}>
					<span className="ai-muted">
						Break 结束：{o.break_ends_at ? formatTime(o.break_ends_at) : "—"}
					</span>
					<span className="ai-muted">
						Fix 开始：{o.fix_started_at ? formatTime(o.fix_started_at) : "—"}
					</span>
					<span className="ai-muted">
						Fix 结束：{o.fix_ends_at ? formatTime(o.fix_ends_at) : "—"}
					</span>
					<span className="ai-muted">
						每回合间隔 {Math.round((o.fix_round_interval_secs ?? 0) / 60)} 分
					</span>
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<div className="ai-row-between">
						<span className="ai-strong" style={{ fontSize: 18 }}>
							题目
						</span>
						<span className="ai-muted">
							{o.phase === "break"
								? "当前是 Break 阶段：先启动实例，再从源码里找漏洞。"
								: o.phase === "fix"
									? "当前是 Fix 阶段：上传补丁，判题器会在后续每个回合重新评测它。"
									: ""}
						</span>
					</div>
					<Divider />
					{o.gameboxes.length === 0 ? (
						<EmptyState
							title="这场赛事还没有题目"
							description="等管理员挂载箱子后再来。"
						/>
					) : (
						o.gameboxes.map((g) => (
							<AwdpGameBoxCard key={g.id} eventId={eventId} box={g} onChanged={() => void overview.refetch()} />
						))
					)}
				</div>
			</Card>

			<Card>
				<Tabs
					aria-label="AWDP 分区"
					defaultActiveKey="rounds"
					items={[
						{
							key: "rounds",
							label: "回合",
							children: <RoundsPanel eventId={eventId} />,
						},
						{
							key: "evaluations",
							label: "我的评测",
							children: <EvaluationsPanel eventId={eventId} />,
						},
						{
							key: "scoreboard",
							label: "积分榜",
							children: <ScoreboardPanel eventId={eventId} />,
						},
						{
							key: "trend",
							label: "趋势",
							children: <TrendPanel eventId={eventId} />,
						},
					]}
				/>
			</Card>
		</>
	);
}

/** 单题卡片：按阶段显示该做的事。 */
function AwdpGameBoxCard({
	eventId,
	box,
	onChanged,
}: {
	eventId: string;
	box: AwdpGameBox;
	onChanged: () => void;
}) {
	const runtime = useRuntime();
	const confirm = useConfirm();
	const [flag, setFlag] = useState("");

	const api = runtime.client.awdp.player;
	const start = useUniMutation(
		() => api.startInstance(eventId, box.id),
		"启动实例",
	);
	const stop = useUniMutation(() => api.stopInstance(eventId, box.id), "停止实例");
	const reset = useUniMutation(() => api.resetInstance(eventId, box.id), "重置实例");
	const submit = useUniMutation(
		(vars: { id: string; egId: string; flag: string }) =>
			api.submitBreak(vars.id, vars.egId, vars.flag),
		"提交 flag",
	);
	const patch = useUniMutation(
		(vars: { id: string; egId: string; file: File }) =>
			api.uploadPatch(vars.id, vars.egId, vars.file),
		"上传补丁",
	);
	const check = useUniMutation(() => api.testCheck(eventId, box.id), "自检");
	// 走 useUniMutation 而不是直接调 API：直接调拿到的是原始 UniResponse，
	// 用它当 URL 会把整个信封（含 code/data）当字符串拼进去。
	const sourceUrl = useUniMutation(() => api.sourceUrl(eventId, box.id), "获取源码链接");

	const inst: AwdpInstance | null = box.instance;

	return (
		<Card>
			<div className="ai-col" style={{ gap: 10 }}>
				<div className="ai-row-between">
					<div className="ai-row" style={{ gap: 8 }}>
						<span className="ai-strong">{box.name}</span>
						<Tag size="small" variant="outlined">
							{box.category}
						</Tag>
						{box.broken ? (
							<Tag size="small" variant="solid" color="app-green">
								<CheckCircleIcon size={12} /> 已攻破
							</Tag>
						) : (
							<Tag size="small" variant="soft" color="app-yellow">
								未攻破
							</Tag>
						)}
						{!box.enabled ? (
							<Tag size="small" variant="soft" color="brown">
								已停用
							</Tag>
						) : null}
					</div>
					{inst ? (
						<Tag
							size="small"
							variant="soft"
							color={/running|up/i.test(inst.runtime_state) ? "app-green" : "brown"}
						>
							实例 {inst.runtime_state}
						</Tag>
					) : null}
				</div>

				{/* 实例生命周期：任何阶段都需要，但目的是不同的。 */}
				<div className="ai-row" style={{ gap: 8, flexWrap: "wrap" }}>
					{!inst ? (
						<Button
							size="small"
							type="primary"
							icon={<PlayIcon size={14} />}
							loading={start.isPending}
							onClick={async () => {
								await start.mutateAsync(undefined);
								Notification.success({ message: "实例已启动" });
								onChanged();
							}}
						>
							启动实例
						</Button>
					) : (
						<>
							<Button
								size="small"
								icon={<StopIcon size={14} />}
								loading={stop.isPending}
								onClick={async () => {
									await stop.mutateAsync(undefined);
									onChanged();
								}}
							>
								停止
							</Button>
							<Button
								size="small"
								danger
								icon={<RefreshIcon size={14} />}
								loading={reset.isPending}
								onClick={async () => {
									const ok = await confirm({
										title: `重置「${box.name}」的实例`,
										consequence:
											"实例会被**重建为初始状态**：你在里面做的修改与已上传的补丁都会丢失（已计分的 break 不会回滚）。已消耗的重置次数：见实例信息。",
										okText: "重置",
										danger: true,
									});
									if (!ok) return;
									await reset.mutateAsync(undefined);
									Notification.success({ message: "实例已重置" });
									onChanged();
								}}
							>
								重置
							</Button>
						</>
					)}
					<Button
						size="small"
						icon={<DownloadIcon size={14} />}
						loading={sourceUrl.isPending}
						onClick={async () => {
							// 源码 URL 是 presigned 链接，后端签发；前端只负责打开。
							const url = await sourceUrl.mutateAsync(undefined);
							if (typeof url === "string" && url) {
								window.open(url, "_blank", "noopener");
							} else {
								Notification.info({ message: "这道题没有提供源码" });
							}
						}}
					>
						下载源码
					</Button>
				</div>

				{inst && inst.endpoints.length > 0 ? (
					<div className="ai-col" style={{ gap: 4 }}>
						{(inst.endpoints ?? []).map((ep) => (
							<div key={`${ep.protocol}-${ep.public_port}`} className="ai-row" style={{ gap: 8 }}>
								<GlobeIcon size={14} />
								<span className="ai-mono">
									{ep.protocol}://{ep.public_host}:{ep.public_port}
								</span>
								<Tag size="small" variant="outlined">
									容器 {ep.container_port}
								</Tag>
							</div>
						))}
						{inst.reset_count > 0 ? (
							<span className="ai-muted" style={{ fontSize: 12 }}>
								已重置 {inst.reset_count} 次
							</span>
						) : null}
					</div>
				) : null}

				<Divider />

				{/* 阶段相关的动作区。 */}
				<div className="ai-col" style={{ gap: 8 }}>
					{box.broken ? (
						<span className="ai-muted">这道题你已经攻破了。重复提交不会重复计分。</span>
					) : (
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
									const r = await submit.mutateAsync({ id: eventId, egId: box.id, flag: flag.trim() });
									if (r?.accepted && r?.scored) {
										Notification.success({ message: "攻破成功", description: "已计入 break 分。" });
									} else if (r?.already_broken) {
										Notification.info({ message: "这道题此前已被你攻破" });
									} else if (r?.accepted) {
										Notification.info({ message: "flag 正确，但本次未计分" });
									} else {
										Notification.error({ message: "flag 不正确" });
									}
									setFlag("");
									onChanged();
								}}
							>
								提交
							</Button>
						</div>
					)}

					<div className="ai-row" style={{ gap: 8, flexWrap: "wrap" }}>
						<Upload
							showUploadList={false}
							accept=".patch,.diff,.zip,.tar.gz"
							customRequest={({ file, onSuccess, onError }) => {
								patch
									.mutateAsync({ id: eventId, egId: box.id, file })
									.then((r) => {
										if (r?.status === "applied") {
											Notification.success({
												message: "补丁已应用",
												description: "它会在后续每个回合被重新评测。",
											});
										} else {
											Notification.error({
												message: "补丁未通过",
												description: r?.error_message ?? "补丁未能应用。",
											});
										}
										onSuccess(r);
										onChanged();
									})
									.catch(onError);
							}}
						>
							<Button icon={<CodeIcon size={14} />}>上传补丁</Button>
						</Upload>
						<Button
							size="small"
							icon={<CheckCircleIcon size={14} />}
							loading={check.isPending}
							onClick={async () => {
								const r = await check.mutateAsync(undefined);
								Notification.info({
									message: `自检：${r?.status ?? "完成"}`,
									description:
										[
											r?.healthcheck_ok === false ? "健康检查未通过" : null,
											r?.judge_ok === false ? "判题未通过" : null,
											r?.exploit_ok === false ? "exploit 仍可打（不计分，仅供参考）" : null,
										]
											.filter(Boolean)
											.join("；") || "各项检查已通过。",
								});
							}}
						>
							手工自检
						</Button>
					</div>
				</div>
			</div>
		</Card>
	);
}

function RoundsPanel({ eventId }: { eventId: string }) {
	const runtime = useRuntime();
	const q = useUniQuery(
		["awdp-rounds", eventId],
		() => runtime.client.awdp.player.rounds(eventId),
		"加载回合",
	);
	if (q.isLoading) return <SkeletonCards count={2} />;
	if (q.error) return <ErrorState error={q.error} onRetry={() => void q.refetch()} />;
	if ((q.data ?? []).length === 0) {
		return <EmptyState title="还没有回合" description="Fix 阶段开始后会出现回合。" />;
	}
	return (
		<div className="ai-col" style={{ gap: 8 }}>
			{(q.data ?? []).map((r) => (
				<div key={r.id} className="ai-row-between">
					<div className="ai-row" style={{ gap: 8 }}>
						<Tag size="small" variant="outlined">
							第 {r.sequence} 回合
						</Tag>
						<Tag
							size="small"
							variant="soft"
							color={r.status === "completed" ? "app-green" : r.status === "active" ? "app-blue" : "brown"}
						>
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

function EvaluationsPanel({ eventId }: { eventId: string }) {
	const runtime = useRuntime();
	const q = useUniQuery(
		["awdp-evaluations", eventId],
		() => runtime.client.awdp.player.evaluations(eventId),
		"加载评测",
	);
	if (q.isLoading) return <SkeletonCards count={3} />;
	if (q.error) return <ErrorState error={q.error} onRetry={() => void q.refetch()} />;
	if ((q.data ?? []).length === 0) {
		return (
			<EmptyState
				title="还没有评测记录"
				description="Fix 阶段每个回合结束后，判题器会产出评测结果。"
			/>
		);
	}
	return (
		<div className="ai-scroll">
			<table style={{ width: "100%", borderCollapse: "collapse" }}>
				<thead>
					<tr>
						{["类型", "回合", "状态", "健康检查", "判题", "完成时间"].map((h) => (
							<th
								key={h}
								style={{
									textAlign: "left",
									padding: "8px 12px",
									borderBottom: "2px solid var(--animal-border-color)",
								}}
							>
								{h}
							</th>
						))}
					</tr>
				</thead>
				<tbody>
					{(q.data ?? []).map((e) => (
						<tr key={e.id}>
							<td style={{ padding: "8px 12px" }}>
								<Tag size="small" variant={e.kind === "official" ? "soft" : "outlined"}>
									{e.kind === "official" ? "官方" : "手工"}
								</Tag>
							</td>
							<td style={{ padding: "8px 12px" }}>{e.round_sequence ?? "—"}</td>
							<td style={{ padding: "8px 12px" }}>{e.status}</td>
							<td style={{ padding: "8px 12px" }}>{e.healthcheck_result ?? "—"}</td>
							<td style={{ padding: "8px 12px" }}>{e.judge_result ?? "—"}</td>
							<td style={{ padding: "8px 12px" }} className="ai-muted">
								{e.finished_at ? formatTime(e.finished_at) : "—"}
							</td>
						</tr>
					))}
				</tbody>
			</table>
		</div>
	);
}

function ScoreboardPanel({ eventId }: { eventId: string }) {
	const runtime = useRuntime();
	const q = useUniQuery(
		["awdp-scoreboard", eventId],
		() => runtime.client.awdp.player.scoreboard(eventId),
		"加载积分榜",
	);
	if (q.isLoading) return <Loading active tip="加载积分榜…" />;
	if (q.error) return <ErrorState error={q.error} onRetry={() => void q.refetch()} />;
	const d = q.data;
	if (!d || d.rows.length === 0) {
		return <EmptyState title="还没有计分主体" description="比赛开始后这里会出现排名。" />;
	}
	return (
		<div className="ai-col" style={{ gap: 12 }}>
			<div className="ai-row" style={{ gap: 8 }}>
				<Tag size="small" variant="outlined">
					{d.participant_mode === "team" ? `${d.rows.length} 支队伍` : `${d.rows.length} 名选手`}
				</Tag>
				<span className="ai-muted" style={{ fontSize: 12 }}>
					共 {d.gameboxes.length} 题 ｜ {d.rounds.length} 个回合
				</span>
			</div>
			<div className="ai-scroll">
				<table style={{ width: "100%", borderCollapse: "collapse" }}>
					<thead>
						<tr>
							<th style={{ textAlign: "left", padding: "8px 12px", borderBottom: "2px solid var(--animal-border-color)" }}>排名</th>
							<th style={{ textAlign: "left", padding: "8px 12px", borderBottom: "2px solid var(--animal-border-color)" }}>名称</th>
							<th style={{ textAlign: "left", padding: "8px 12px", borderBottom: "2px solid var(--animal-border-color)" }}>Break</th>
							<th style={{ textAlign: "left", padding: "8px 12px", borderBottom: "2px solid var(--animal-border-color)" }}>Fix</th>
							<th style={{ textAlign: "left", padding: "8px 12px", borderBottom: "2px solid var(--animal-border-color)" }}>总分</th>
							{d.gameboxes.map((g) => (
								<th key={g.id} style={{ textAlign: "center", padding: "8px 12px", borderBottom: "2px solid var(--animal-border-color)" }}>
									<Tooltip title={g.name}>
										<span style={{ fontSize: 12 }}>{g.name.slice(0, 6)}</span>
									</Tooltip>
								</th>
							))}
						</tr>
					</thead>
					<tbody>
						{d.rows.map((row) => (
							<tr key={row.subject_id} style={row.is_me ? { background: "var(--animal-primary-bg)" } : undefined}>
								<td style={{ padding: "8px 12px" }}>
									<Tag size="small" variant="soft">#{row.rank}</Tag>
								</td>
								<td style={{ padding: "8px 12px" }} className="ai-strong">
									{row.subject_name}
									{row.is_me ? "（你）" : ""}
								</td>
								<td style={{ padding: "8px 12px" }}>{row.break_score}</td>
								<td style={{ padding: "8px 12px" }}>{row.fix_score}</td>
								<td style={{ padding: "8px 12px" }} className="ai-strong">{row.total_score}</td>
								{d.gameboxes.map((g, i) => (
									<td key={g.id} style={{ padding: "8px 12px", textAlign: "center" }}>
										{row.break_status?.[i] ? (
											<Tag size="small" variant="solid" color="app-green">破</Tag>
										) : (
											<span className="ai-muted">·</span>
										)}
										{row.fix_gamebox_score?.[i] ? (
											<span className="ai-muted" style={{ fontSize: 12 }}> +{row.fix_gamebox_score[i]}</span>
										) : null}
									</td>
								))}
							</tr>
						))}
					</tbody>
				</table>
			</div>
		</div>
	);
}

function TrendPanel({ eventId }: { eventId: string }) {
	const runtime = useRuntime();
	const q = useUniQuery(
		["awdp-trend", eventId],
		() => runtime.client.awdp.player.trend(eventId),
		"加载趋势",
	);
	if (q.isLoading) return <SkeletonCards count={2} />;
	if (q.error) return <ErrorState error={q.error} onRetry={() => void q.refetch()} />;
	if ((q.data ?? []).length === 0) {
		return <EmptyState title="还没有趋势数据" description="有人得分后这里会出现曲线数据。" />;
	}
	// 没有引入图表库（设计法也只允许 naive-icons），因此不自绘 SVG，
	// 而是给出可读的**得分增长序列**：每一行是一个计分事件。
	return (
		<div className="ai-col" style={{ gap: 10 }}>
			<span className="ai-muted" style={{ fontSize: 12 }}>
				各主体的得分增长序列。本项目未引入图表库，因此以时序表格呈现，而不是自绘图形。
			</span>
			{(q.data ?? []).map((t) => {
				const last = t.points[t.points.length - 1];
				return (
					<Card key={t.name}>
						<div className="ai-col" style={{ gap: 8 }}>
							<div className="ai-row-between">
								<span className="ai-strong">{t.name}</span>
								<div className="ai-row" style={{ gap: 8 }}>
									<Tag size="small" variant="outlined">
										{t.points.length} 次计分
									</Tag>
									<Tag size="small" variant="soft" color="app-teal">
										最终 {last?.score ?? 0} 分
									</Tag>
								</div>
							</div>
							<div className="ai-scroll" style={{ maxHeight: 200 }}>
								<table style={{ width: "100%", borderCollapse: "collapse" }}>
									<tbody>
										{t.points.map((pt, i) => (
											<tr key={`${pt.time}-${i}`}>
												<td style={{ padding: "4px 8px" }} className="ai-muted">
													{formatTime(pt.time)}
												</td>
												<td style={{ padding: "4px 8px" }}>{pt.name}</td>
												<td style={{ padding: "4px 8px", textAlign: "right" }} className="ai-strong">
													{pt.score}
												</td>
											</tr>
										))}
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
