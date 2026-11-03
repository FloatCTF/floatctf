/**
 * AWDP 管理端控制台。
 *
 * ## 阶段推进是单调的
 *
 * `pending → break → preparing_fix → fix → ended`。管理员的动作只有三个：
 * **开始**（进入 break）、**转入 Fix**（break → fix）、**结束**。界面据此把可执行的动作
 * 收敛成"当前状态下的下一步"，而不是把三个按钮一直摆着 —— 后者会让人以为可以任意跳阶段。
 *
 * ## 配置同样是乐观锁
 *
 * `updateConfig` 收 `expected_updated_at`。两个管理员同时改时长/分值时，
 * 后保存的人收到冲突而不是静默覆盖。因此冲突时提示重新加载，**不重试覆盖**。
 */
import { useState } from "react";
import { useNavigate, useParams } from "react-router";
import {
	Button,
	Card,
	Divider,
	Input,
	Notification,
	Select,
	Tabs,
	Tag,
	Tooltip,
} from "animal-island-ui";
import {
	AlertTriangleIcon,
	CheckCircleIcon,
	ClockIcon,
	GlobeIcon,
	PlayIcon,
	RefreshIcon,
	ShieldIcon,
	StopIcon,
	WifiIcon,
} from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { Can } from "~/app/permissions";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { EmptyState, ErrorState, LoadingState, SkeletonCards } from "~/components/states";
import { StatCard } from "~/components/resource";
import { formatTime } from "~/lib/event";
import { createBindings } from "~/api/react";
import type { AwdpConfigPatchInput } from "@floatctf/sdk";

const PHASE_LABEL: Record<string, string> = {
	pending: "未开始",
	break: "Break（攻破）",
	preparing_fix: "准备修复",
	fix: "Fix（修复）",
	ended: "已结束",
};

export function ConsoleAwdpEvent() {
	const { eventId = "" } = useParams();
	const runtime = useRuntime();
	const navigate = useNavigate();
	const confirm = useConfirm();

	const bindings = useState(() => createBindings(runtime.client))[0];
	const stream = bindings.useAwdpEventStream({ eventId, enabled: Boolean(eventId) });

	const config = useUniQuery(
		["awdp-admin-config", eventId],
		() => runtime.client.awdp.admin.getConfig(eventId),
		"加载 AWDP 配置",
		{ enabled: Boolean(eventId) },
	);

	const api = runtime.client.awdp.admin;
	const start = useUniMutation(() => api.start(eventId), "开始比赛");
	const toFix = useUniMutation(() => api.breakToFix(eventId), "转入修复");
	const finish = useUniMutation(() => api.finish(eventId), "结束比赛");

	if (!eventId) {
		return (
			<Card>
				<EmptyState
					title="没有指定赛事"
					description="AWDP 控制台必须绑定到一场赛事。"
					action={<Button onClick={() => void navigate("/console/events")}>去赛事管理</Button>}
				/>
			</Card>
		);
	}

	if (config.isLoading) return <LoadingState label="加载 AWDP 配置…" />;
	if (config.error) {
		return <ErrorState error={config.error} onRetry={() => void config.refetch()} />;
	}
	if (!config.data) {
		return (
			<Card>
				<EmptyState
					title="尚未初始化 AWDP"
					description="这场赛事还没有 AWDP 配置。请先在赛事里创建 AWDP 并挂载箱子。"
					action={
						<Button onClick={() => void navigate(`/console/events/${eventId}`)}>
							回赛事详情
						</Button>
					}
				/>
			</Card>
		);
	}

	const c = config.data;
	const phase = c.phase;

	return (
		<>
			<PageHeader
				title="AWDP 控制台"
				description={`阶段 ${PHASE_LABEL[phase] ?? phase}｜回合 ${c.current_round ?? 0}/${c.total_rounds ?? 0}`}
				extra={
					<>
						<Tooltip title={stream.connected ? "事件流已连接" : "事件流不可用，正在轮询兜底"}>
							<Tag
								size="small"
								variant="soft"
								color={stream.connected ? "app-green" : "app-yellow"}
							>
								<WifiIcon size={12} />
								{stream.connected ? "实时" : "轮询兜底"}
							</Tag>
						</Tooltip>
						<Button
							size="small"
							icon={<RefreshIcon size={16} />}
							onClick={() => void config.refetch()}
						>
							刷新
						</Button>
					</>
				}
			/>

			<div className="ai-grid ai-grid-wide">
				<StatCard label="阶段" value={<span style={{ fontSize: 16 }}>{PHASE_LABEL[phase] ?? phase}</span>} />
				<StatCard label="当前回合" value={`${c.current_round ?? 0}/${c.total_rounds ?? 0}`} icon={<ClockIcon size={18} />} />
				<StatCard label="Break 单题分" value={c.break_score} />
				<StatCard label="Fix 每回合分" value={c.fix_round_score} />
				<StatCard label="配置代数" value={c.configuration_generation} hint="每次改配置 +1" />
				<StatCard
					label="Break 结束"
					value={<span style={{ fontSize: 14 }}>{c.break_ends_at ? formatTime(c.break_ends_at) : "—"}</span>}
				/>
			</div>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<span className="ai-strong" style={{ fontSize: 18 }}>
						阶段推进
					</span>
					<span className="ai-muted">
						阶段是**单调**的：只能从开始推进到 Break，再转入 Fix，最后结束；不能跳回上一阶段。
					</span>
					<Divider />
					<Can permission="events.lifecycle.manage">
					<div className="ai-row" style={{ gap: 8, flexWrap: "wrap" }}>
						<Button
							type={phase === "pending" ? "primary" : "default"}
							icon={<PlayIcon size={14} />}
							disabled={phase !== "pending"}
							loading={start.isPending}
							onClick={async () => {
								const ok = await confirm({
									title: "开始 AWDP 比赛",
									consequence:
										"进入 **Break 阶段**：开始计时，选手可以启动实例并提交 flag。开始后不能回到未开始状态。",
									okText: "开始比赛",
									danger: true,
								});
								if (!ok) return;
								await start.mutateAsync(undefined);
								Notification.success({ message: "比赛已开始" });
								void config.refetch();
							}}
						>
							开始比赛
						</Button>

						<Button
							type={phase === "break" ? "primary" : "default"}
							icon={<CheckCircleIcon size={14} />}
							disabled={phase !== "break"}
							loading={toFix.isPending}
							onClick={async () => {
								const ok = await confirm({
									title: "结束 Break，转入 Fix",
									consequence:
										"**Break 阶段立即结束**，选手不能再提交 flag。之后进入修复阶段，按回合对补丁评分。这是不可逆的阶段切换。",
									okText: "转入 Fix",
									danger: true,
									confirmPhrase: "转入",
								});
								if (!ok) return;
								await toFix.mutateAsync(undefined);
								Notification.success({ message: "已转入 Fix" });
								void config.refetch();
							}}
						>
							转入 Fix
						</Button>

						<Button
							danger
							icon={<StopIcon size={14} />}
							disabled={phase === "ended"}
							loading={finish.isPending}
							onClick={async () => {
								const ok = await confirm({
									title: "结束 AWDP 比赛",
									consequence:
										"比赛立即结束：**停止回合计时与全部评测，最终排名定格**。结束后不能恢复，也不能补提交。",
									okText: "结束比赛",
									danger: true,
									confirmPhrase: "结束",
								});
								if (!ok) return;
								await finish.mutateAsync(undefined);
								Notification.success({ message: "比赛已结束" });
								void config.refetch();
							}}
						>
							结束比赛
						</Button>
					</div>
					</Can>
					{phase === "pending" && (c.total_rounds ?? 0) === 0 ? (
						<div className="ai-row" style={{ gap: 8 }}>
							<AlertTriangleIcon size={16} />
							<span className="ai-muted">
								回合总数为 0 —— 开赛前请先在「参数配置」里设置总回合数。
							</span>
						</div>
					) : null}
				</div>
			</Card>

			<Card>
				<Tabs
					aria-label="AWDP 管理分区"
					defaultActiveKey="config"
					items={[
						{
							key: "config",
							label: "参数配置",
							children: <ConfigPanel eventId={eventId} current={c} onSaved={() => void config.refetch()} />,
						},
						{
							key: "gameboxes",
							label: "题目挂载",
							children: <GameboxesPanel eventId={eventId} />,
						},
						{
							key: "instances",
							label: "实例",
							children: <InstancesPanel eventId={eventId} />,
						},
						{
							key: "scores",
							label: "积分榜",
							children: <ScoresPanel eventId={eventId} />,
						},
						{
							key: "overview",
							label: "总览数据",
							children: <OverviewPanel eventId={eventId} />,
						},
					]}
				/>
			</Card>
		</>
	);
}

function ConfigPanel({
	eventId,
	current,
	onSaved,
}: {
	eventId: string;
	current: { updated_at: string; break_duration_secs: number; fix_duration_secs: number; fix_round_interval_secs: number; break_score: number; fix_round_score: number; total_rounds?: number; phase?: string };
	onSaved: () => void;
}) {
	const runtime = useRuntime();
	const [draft, setDraft] = useState<AwdpConfigPatchInput>({});

	const save = useUniMutation(
		(vars: { eventId: string; body: AwdpConfigPatchInput }) =>
			runtime.client.awdp.admin.updateConfig(vars.eventId, vars.body),
		"保存配置",
	);

	const num = (raw: unknown, fallback: number): number => {
		const n = typeof raw === "number" ? raw : Number(String(raw ?? "").trim());
		return Number.isFinite(n) ? n : fallback;
	};

	const field = (label: string, key: keyof AwdpConfigPatchInput, fallback: number, hint?: string) => (
		<label className="ai-col" style={{ gap: 6 }}>
			<span>{label}</span>
			<Input
				value={String(draft[key] ?? "")}
				placeholder={String(fallback)}
				onChange={(e) => setDraft({ ...draft, [key]: e.target.value as never })}
			/>
			{hint ? (
				<span className="ai-muted" style={{ fontSize: 12 }}>
					{hint}
				</span>
			) : null}
		</label>
	);

	return (
		<div className="ai-col" style={{ gap: 12 }}>
			<span className="ai-muted">
				留空表示保持当前值。保存会带上**加载时那一版的时间戳**（乐观锁）。
				`total_rounds` 由后端从回合表推导，不在这里改。
			</span>
			<div className="ai-grid">
				{field("Break 时长（秒）", "break_duration_secs", current.break_duration_secs)}
				{field("每回合 Fix 时长（秒）", "fix_duration_secs", current.fix_duration_secs)}
				{field(
					"回合间隔（秒）",
					"fix_round_interval_secs",
					current.fix_round_interval_secs,
					"判题与下一回合之间的间隔",
				)}
				{field("Break 单题分", "break_score", current.break_score)}
				{field(
					"Fix 每回合分",
					"fix_round_score",
					current.fix_round_score,
					"补丁每撑住一个回合的收益",
				)}
			</div>
			<div className="ai-row" style={{ justifyContent: "flex-end" }}>
				<Can permission="awdp.manage">
				<Button
					type="primary"
					loading={save.isPending}
					disabled={Object.keys(draft).length === 0}
					onClick={async () => {
						const body: AwdpConfigPatchInput = { ...draft };
						for (const [k, v] of Object.entries(body)) {
							if (k === "expected_updated_at") continue;
							if (typeof v === "string" && v.trim() === "") {
								delete (body as Record<string, unknown>)[k];
							} else if (typeof v === "string") {
								(body as Record<string, unknown>)[k] = num(v, 0);
							}
						}
						body.expected_updated_at = current.updated_at;
						try {
							await save.mutateAsync({ eventId, body });
							Notification.success({ message: "配置已保存" });
							setDraft({});
							onSaved();
						} catch (e) {
							// 冲突不重试覆盖。
							Notification.error({
								message: "保存失败",
								description:
									e instanceof Error
										? `${e.message}（如果是冲突，请刷新后重新修改）`
										: "请刷新后重试。",
							});
						}
					}}
				>
					保存配置
				</Button>
				</Can>
			</div>
		</div>
	);
}

/** 题目挂载：需要从箱子库里挑 AWDP-capable 的箱子。 */
function GameboxesPanel({ eventId }: { eventId: string }) {
	const runtime = useRuntime();
	const confirm = useConfirm();

	const mounted = useUniQuery(
		["awdp-admin-gameboxes", eventId],
		() => runtime.client.awdp.admin.listEventGameboxes(eventId),
		"加载已挂载题目",
	);
	// 用 AWDP 目录而不是箱子库：只有它带 `awdp_capable` 标记
	// （`GameBoxLibraryDto` 没有这个字段 —— 挂了不支持的箱子在开赛时才会炸）。
	const catalog = useUniQuery(
		["awdp-catalog"],
		() => runtime.client.awdp.runs.gameboxCatalog(),
		"加载 AWDP 箱子目录",
	);

	const attach = useUniMutation(
		(vars: { gameboxId: string }) => runtime.client.awdp.admin.attachGamebox(eventId, vars.gameboxId),
		"挂载题目",
	);
	const detach = useUniMutation(
		(vars: { egId: string }) => runtime.client.awdp.admin.detachGamebox(eventId, vars.egId),
		"卸载题目",
	);

	if (mounted.isLoading || catalog.isLoading) return <SkeletonCards count={3} />;
	if (mounted.error) {
		return <ErrorState error={mounted.error} onRetry={() => void mounted.refetch()} />;
	}

	const mountedIds = new Set((mounted.data ?? []).map((m) => m.gamebox_id));
	// 只列出支持 AWDP 且尚未挂载的。
	const available = (catalog.data ?? []).filter(
		(g) => !mountedIds.has(g.id) && g.awdp_capable,
	);

	return (
		<div className="ai-col" style={{ gap: 12 }}>
			<Card>
				<div className="ai-row-between">
					<div className="ai-col" style={{ gap: 2, alignItems: "flex-start" }}>
						<span className="ai-strong">从箱子库挂载</span>
						<span className="ai-muted" style={{ fontSize: 12 }}>
							只有**支持 AWDP** 的箱子（含源码目录）才有意义。
						</span>
					</div>
					<Can permission="awdp.manage">
					<div style={{ minWidth: 300 }}>
						<Select
							value=""
							placeholder="选择一个箱子加入本场"
							showSearch
							options={available.map((g) => ({
								key: g.id,
								label: `${g.name}${g.version ? ` v${g.version}` : ""}`,
							}))}
							onChange={async (v) => {
								if (typeof v !== "string" || !v) return;
								await attach.mutateAsync({ gameboxId: v });
								Notification.success({ message: "已挂载" });
								void mounted.refetch();
							}}
						/>
					</div>
					</Can>
				</div>
			</Card>

			{(mounted.data ?? []).length === 0 ? (
				<Card>
					<EmptyState
						title="还没有挂载题目"
						description="没有题目的 AWDP 比赛无法进行。"
					/>
				</Card>
			) : (
				(mounted.data ?? []).map((m) => (
					<Card key={m.id}>
						<div className="ai-col" style={{ gap: 8 }}>
							<div className="ai-row-between">
								<div className="ai-row" style={{ gap: 8 }}>
									<span className="ai-strong">{m.name}</span>
									<Tag size="small" variant="outlined">
										{m.category}
									</Tag>
									{!m.awdp_capable ? (
										<Tag size="small" variant="soft" color="app-red">
											<AlertTriangleIcon size={12} /> 不支持 AWDP
										</Tag>
									) : null}
									{m.hidden ? (
										<Tag size="small" variant="soft" color="brown">
											对选手隐藏
										</Tag>
									) : null}
									{m.enabled ? (
										<Tag size="small" variant="soft" color="app-green">
											启用
										</Tag>
									) : (
										<Tag size="small" variant="soft" color="app-red">
											停用
										</Tag>
									)}
								</div>
								<Can permission="awdp.manage">
								<Button
									size="small"
									danger
									icon={<StopIcon size={14} />}
									onClick={async () => {
										const ok = await confirm({
											title: `从本场卸载「${m.name}」`,
											consequence:
												"该题目会从本场 AWDP 中移除，**对应实例在选手侧不再可见**。已产生的分数与评测记录保留（它们属于历史）。",
											okText: "卸载",
											danger: true,
										});
										if (!ok) return;
										await detach.mutateAsync({ egId: m.id });
										Notification.success({ message: "已卸载" });
										void mounted.refetch();
									}}
								>
									卸载
								</Button>
								</Can>
							</div>
							<div className="ai-row" style={{ gap: 16, flexWrap: "wrap" }}>
								<span className="ai-muted" style={{ fontSize: 12 }}>
									CPU {m.cpu_millis} ‰ ｜ 内存 {Math.round((m.memory_bytes ?? 0) / 1024 / 1024)} MiB
								</span>
								<span className="ai-muted ai-mono" style={{ fontSize: 12 }}>
									源码目录 {m.awdp_source_code_dir ?? "—"}
								</span>
								<span className="ai-muted" style={{ fontSize: 12 }}>
									构建 {m.build_status ?? "未知"}
								</span>
							</div>
						</div>
					</Card>
				))
			)}
		</div>
	);
}

function InstancesPanel({ eventId }: { eventId: string }) {
	const runtime = useRuntime();
	const q = useUniQuery(
		["awdp-admin-instances", eventId],
		() => runtime.client.awdp.admin.listInstances(eventId),
		"加载实例",
	);
	if (q.isLoading) return <SkeletonCards count={3} />;
	if (q.error) return <ErrorState error={q.error} onRetry={() => void q.refetch()} />;
	if ((q.data ?? []).length === 0) {
		return <EmptyState title="还没有实例" description="选手启动实例后会出现在这里。" />;
	}
	return (
		<div className="ai-col" style={{ gap: 8 }}>
			{(q.data ?? []).map((i) => (
				<div key={i.instance_id} className="ai-row-between">
					<div className="ai-col" style={{ gap: 2, alignItems: "flex-start" }}>
						<span className="ai-strong">{i.gamebox_name}</span>
						<span className="ai-muted ai-mono" style={{ fontSize: 12 }}>
							{i.container_name}
						</span>
					</div>
					<div className="ai-row" style={{ gap: 8 }}>
						{i.endpoints.map((ep) => (
							<Tag key={`${ep.protocol}-${ep.public_port}`} size="small" variant="outlined">
								{ep.public_host}:{ep.public_port}
							</Tag>
						))}
						<Tag
							size="small"
							variant="soft"
							color={/running|up/i.test(i.runtime_state) ? "app-green" : "brown"}
						>
							{i.runtime_state} (gen {i.runtime_generation})
						</Tag>
					</div>
				</div>
			))}
		</div>
	);
}

function ScoresPanel({ eventId }: { eventId: string }) {
	const runtime = useRuntime();
	const q = useUniQuery(
		["awdp-admin-scores", eventId],
		() => runtime.client.awdp.admin.scores(eventId),
		"加载积分榜",
	);
	if (q.isLoading) return <SkeletonCards count={3} />;
	if (q.error) return <ErrorState error={q.error} onRetry={() => void q.refetch()} />;
	if ((q.data ?? []).length === 0) {
		return <EmptyState title="还没有计分主体" />;
	}
	return (
		<div className="ai-scroll">
			<table style={{ width: "100%", borderCollapse: "collapse" }}>
				<thead>
					<tr>
						{["排名", "名称", "Break", "Fix", "总分"].map((h) => (
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
					{(q.data ?? []).map((row) => (
						<tr key={row.subject_id}>
							<td style={{ padding: "8px 12px" }}>
								<Tag size="small" variant="soft">
									#{row.rank}
								</Tag>
							</td>
							<td style={{ padding: "8px 12px" }} className="ai-strong">
								{row.subject_name}
							</td>
							<td style={{ padding: "8px 12px" }}>{row.break_score}</td>
							<td style={{ padding: "8px 12px" }}>{row.fix_score}</td>
							<td style={{ padding: "8px 12px" }} className="ai-strong">
								{row.total_score}
							</td>
						</tr>
					))}
				</tbody>
			</table>
		</div>
	);
}

function OverviewPanel({ eventId }: { eventId: string }) {
	const runtime = useRuntime();
	const q = useUniQuery(
		["awdp-admin-datapresent", eventId],
		() => runtime.client.awdp.admin.dataPresent(eventId),
		"加载总览数据",
	);
	if (q.isLoading) return <LoadingState label="加载总览…" />;
	if (q.error) return <ErrorState error={q.error} onRetry={() => void q.refetch()} />;
	const d = q.data;
	if (!d) return <EmptyState title="没有总览数据" />;
	return (
		<div className="ai-col" style={{ gap: 12 }}>
			<div className="ai-grid">
				<StatCard label="参与人数" value={d.user_count} icon={<ShieldIcon size={18} />} />
				<StatCard label="队伍数" value={d.team_count} />
				<StatCard label="题目数" value={d.gameboxes.length} icon={<GlobeIcon size={18} />} />
				<StatCard label="最近动态" value={d.recent_activity.length} />
			</div>

			<Card>
				<div className="ai-col" style={{ gap: 10 }}>
					<span className="ai-strong">Top 10</span>
					<Divider />
					{d.scoreboard_top10.length === 0 ? (
						<EmptyState title="还没有排名" />
					) : (
						d.scoreboard_top10.map((r) => (
							<div key={r.subject_id} className="ai-row-between">
								<div className="ai-row" style={{ gap: 8 }}>
									<Tag size="small" variant="soft">
										#{r.rank}
									</Tag>
									<span>{r.subject_name}</span>
								</div>
								<span className="ai-strong">{r.total_score}</span>
							</div>
						))
					)}
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 10 }}>
					<span className="ai-strong">最近动态</span>
					<Divider />
					{d.recent_activity.length === 0 ? (
						<EmptyState title="还没有动态" />
					) : (
						d.recent_activity.map((a, i) => (
							<div key={`${a.subject_name}-${a.created_at}-${i}`} className="ai-row-between">
								<div className="ai-row" style={{ gap: 8 }}>
									<Tag
										size="small"
										variant="soft"
										color={a.action === "break" ? "app-red" : "app-green"}
									>
										{a.action === "break" ? "破题" : "修复"}
									</Tag>
									<span className="ai-strong">{a.subject_name}</span>
									<span className="ai-muted">{a.gamebox_name}</span>
								</div>
								<div className="ai-row" style={{ gap: 8 }}>
									<span className="ai-strong">
										{a.delta >= 0 ? "+" : ""}
										{a.delta}
									</span>
									<span className="ai-muted" style={{ fontSize: 12 }}>
										{formatTime(a.created_at)}
									</span>
								</div>
							</div>
						))
					)}
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 10 }}>
					<span className="ai-strong">题目统计</span>
					<Divider />
					{d.gameboxes.length === 0 ? (
						<EmptyState title="没有题目统计" />
					) : (
						d.gameboxes.map((g) => (
							<div key={g.id} className="ai-row-between">
								<div className="ai-row" style={{ gap: 8 }}>
									<span>{g.name}</span>
									<Tag size="small" variant="outlined">
										{g.category}
									</Tag>
								</div>
								<div className="ai-row" style={{ gap: 8 }}>
									<Tag size="small" variant="soft" color="app-red">
										破 {g.break_count}
									</Tag>
									<Tag size="small" variant="soft" color="app-green">
										修复 {g.fix_count}
									</Tag>
								</div>
							</div>
						))
					)}
				</div>
			</Card>
		</div>
	);
}
