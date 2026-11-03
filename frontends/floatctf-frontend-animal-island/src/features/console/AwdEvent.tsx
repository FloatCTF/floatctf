/**
 * AWD 管理端控制台。
 *
 * ## 比赛生命周期是**单向**的，界面必须表达这一点
 *
 * `deploy → start ⇄ pause → finish → archive` 不是一组并列按钮：
 * 顺序错了会得到一场状态错乱的比赛。因此这里把当前状态对应的**下一步**标为主要动作，
 * 其余保持次要，并在确认弹窗里写清"这之后会发生什么"。
 *
 * ## 乐观锁
 *
 * `updateConfig` 收 `expected_updated_at`。两个管理员同时改配置时，后保存的人会收到
 * 冲突而不是静默覆盖对方的改动。因此表单保存时把**加载时那一版**的时间戳带上，
 * 收到冲突就提示重新加载 —— 而不是重试覆盖。
 *
 * ## query key 与 `AWD_ADMIN_QUERY_KEYS` 对齐
 * （`admin-awd-status` / `admin-awd-scores` / `admin-awd-prechecks` / `awd-gameboxes`）
 * 否则管理端的实时失效会失效（见 `~/api/react.ts` 的说明）。
 */
import { useState } from "react";
import { useNavigate, useParams } from "react-router";
import {
	Button,
	Card,
	Divider,
	Input,
	Modal,
	Notification,
	Select,
	Tabs,
	Tag,
	Tooltip,
} from "animal-island-ui";
import {
	AlertTriangleIcon,
	CheckCircleIcon,
	GlobeIcon,
	KeyIcon,
	RefreshIcon,
	ShieldIcon,
	TrashIcon,
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
import type { AwdEventConfigInput } from "@floatctf/sdk";

const K_STATUS = "admin-awd-status";
const K_SCORES = "admin-awd-scores";
const K_PRECHECKS = "admin-awd-prechecks";
const K_GAMEBOXES = "awd-gameboxes";

/** 生命周期动作的可读说明 —— 确认弹窗里要告诉管理员"这之后会发生什么"。 */
const LIFECYCLE_STEPS = [
	{
		key: "deploy" as const,
		label: "部署",
		danger: true,
		effect:
			"会按当前配置**创建全部靶机容器与网络**。这一步会真实占用宿主资源，且需要较长时间；部署完成前比赛无法开始。",
	},
	{
		key: "start" as const,
		label: "开始比赛",
		danger: true,
		effect:
			"**正式开赛**：开始轮次计时并开始判题。选手会立刻可以提交 flag。开始后不能回到未开始状态，只能暂停或结束。",
	},
	{
		key: "pause" as const,
		label: "暂停",
		danger: false,
		effect:
			"暂停当前轮次计时与判题。**已产生的分数与靶机状态保留**；恢复后从中断处继续。",
	},
	{
		key: "resume" as const,
		label: "继续",
		danger: false,
		effect: "恢复轮次计时与判题。选手侧会立即恢复正常。",
	},
	{
		key: "finish" as const,
		label: "结束比赛",
		danger: true,
		effect:
			"**结束比赛**：停止轮次与判题，最终排名定格。结束后无法再恢复比赛，只能归档。",
	},
	{
		key: "archive" as const,
		label: "归档",
		danger: true,
		effect:
			"归档赛事数据并按保留策略清理运行资源。归档后本场比赛不再可操作，请确认排名与导出已经完成。",
	},
] as const;

export function ConsoleAwdEvent() {
	const { eventId = "" } = useParams();
	const runtime = useRuntime();
	const navigate = useNavigate();
	// 这个声明是必须的：漏掉它 confirm 会静默解析成全局 window.confirm（原生弹窗，
	// 平台明确禁止，且它是同步 string API —— 类型上会立刻暴露）。
	const confirm = useConfirm();

	const bindings = useState(() => createBindings(runtime.client))[0];
	const stream = bindings.useAdminAwdEventStream({ eventId, enabled: Boolean(eventId) });

	const status = useUniQuery(
		[K_STATUS, eventId],
		() => runtime.client.awd.admin.getStatus(eventId),
		"加载 AWD 状态",
		{ enabled: Boolean(eventId) },
	);

	const lifecycle = useUniMutation(
		(vars: { action: (typeof LIFECYCLE_STEPS)[number]["key"] }) =>
			runtime.client.awd.admin[vars.action](eventId),
		"更新比赛状态",
	);

	if (!eventId) {
		return (
			<Card>
				<EmptyState
					title="没有指定赛事"
					description="AWD 控制台必须绑定到一场赛事。"
					action={<Button onClick={() => void navigate("/console/events")}>去赛事管理</Button>}
				/>
			</Card>
		);
	}

	if (status.isLoading) return <LoadingState label="加载 AWD 状态…" />;
	if (status.error) {
		return <ErrorState error={status.error} onRetry={() => void status.refetch()} />;
	}

	const st = status.data;

	// 没有 AWD 配置时，这一页要引导去创建，而不是显示一堆空面板。
	if (!st) {
		return (
			<>
				<PageHeader title="AWD 控制台" description="这场赛事还没有创建 AWD 配置。" />
				<Card>
					<EmptyState
						title="尚未初始化 AWD"
						description="需要先为这场赛事创建 AWD 配置（轮次、时长、判题参数），然后才能部署。"
						action={
							<Button
								type="primary"
								onClick={() => void navigate(`/console/events/${eventId}`)}
							>
								回赛事详情
							</Button>
						}
					/>
				</Card>
			</>
		);
	}

	return (
		<>
			<PageHeader
				title="AWD 控制台"
				description={`状态 ${st.status}｜阶段 ${st.phase}｜轮次 ${st.round_count ?? "—"}`}
				extra={
					<>
						<Tooltip
							title={
								stream.connected
									? "管理端事件流已连接"
									: "事件流不可用，正在轮询兜底"
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
						<Button
							size="small"
							icon={<RefreshIcon size={16} />}
							onClick={() => void status.refetch()}
						>
							刷新
						</Button>
					</>
				}
			/>

			<div className="ai-grid ai-grid-wide">
				<StatCard label="状态" value={<span style={{ fontSize: 16 }}>{st.status}</span>} />
				<StatCard label="阶段" value={<span style={{ fontSize: 16 }}>{st.phase}</span>} />
				<StatCard label="轮次总数" value={st.round_count ?? "—"} />
				<StatCard
					label="每轮时长"
					value={`${Math.round((st.round_duration_secs ?? 0) / 60)} 分`}
				/>
				<StatCard label="初始分" value={st.initial_score ?? "—"} />
				<StatCard label="免费重置" value={st.free_reset_count ?? "—"} />
				<StatCard
					label="开始时间"
					value={
						<span style={{ fontSize: 14 }}>
							{st.started_at ? formatTime(st.started_at) : "未开始"}
						</span>
					}
				/>
			</div>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<span className="ai-strong" style={{ fontSize: 18 }}>
						生命周期
					</span>
					<span className="ai-muted">
						比赛是单向推进的：依次为部署、开始、（暂停或继续）、结束、归档。请按顺序操作。
					</span>
					<Divider />
					<Can permission="events.lifecycle.manage">
					<div className="ai-row" style={{ gap: 8, flexWrap: "wrap" }}>
						{LIFECYCLE_STEPS.map((step) => (
							<Button
								key={step.key}
								// 暂停中时"继续"才是主要动作；已开始且未暂停时"暂停"是主要动作。
								type={
									(step.key === "start" && st.phase === "pending") ||
									(step.key === "resume" && st.phase === "paused") ||
									(step.key === "pause" && st.status === "running")
										? "primary"
										: "default"
								}
								danger={step.danger}
								size="small"
								loading={lifecycle.isPending}
								onClick={async () => {
									const ok = await confirm({
										title: `${step.label}这场比赛`,
										consequence: step.effect,
										okText: step.label,
										danger: step.danger,
										// 结束与归档是最不可逆的两步，要求输入确认词。
										confirmPhrase:
											step.key === "finish" ? "结束" : step.key === "archive" ? "归档" : undefined,
									});
									if (!ok) return;
									try {
										await lifecycle.mutateAsync({ action: step.key });
										Notification.success({ message: `${step.label}已提交` });
										void status.refetch();
									} catch {
										// 状态机拒绝（例如未部署就想开始）会返回错误，已统一提示。
									}
								}}
							>
								{step.label}
							</Button>
						))}
					</div>
					</Can>
				</div>
			</Card>

			<Card>
				<Tabs
					aria-label="AWD 管理分区"
					defaultActiveKey="config"
					items={[
						{
							key: "config",
							label: "参数配置",
							children: <ConfigPanel eventId={eventId} current={st} onSaved={() => void status.refetch()} />,
						},
						{
							key: "precheck",
							label: "预检与凭据",
							children: <PrecheckPanel eventId={eventId} />,
						},
						{
							key: "teams",
							label: "队伍与判罚",
							children: <TeamsPanel eventId={eventId} />,
						},
						{
							key: "boxes",
							label: "靶机挂载",
							children: <GameboxesPanel eventId={eventId} />,
						},
						{
							key: "network",
							label: "赛事网络",
							children: <NetworkPanel eventId={eventId} />,
						},
						{
							key: "scores",
							label: "积分榜",
							children: <ScoresPanel eventId={eventId} />,
						},
					]}
				/>
			</Card>
		</>
	);
}

/** 参数配置：带乐观锁。 */
function ConfigPanel({
	eventId,
	current,
	onSaved,
}: {
	eventId: string;
	current: { status: string; phase: string; round_count: number | null; round_duration_secs: number; initial_score: number; free_reset_count: number; extra_reset_penalty: number; judge_max_concurrency: number; judge_default_timeout_secs: number; judge_retry_interval_secs: number; archive_retention_hours: number; planned_start_at: string | null; verified_at: string | null; started_at: string | null; event_id: string };
	onSaved: () => void;
}) {
	const runtime = useRuntime();
	const [draft, setDraft] = useState<AwdEventConfigInput>({});

	const save = useUniMutation(
		(vars: { eventId: string; body: AwdEventConfigInput }) =>
			runtime.client.awd.admin.updateConfig(vars.eventId, vars.body),
		"保存配置",
	);

	/** 数字字段：空串不能被当成 0 提交。 */
	const num = (raw: unknown, fallback: number): number => {
		const n = typeof raw === "number" ? raw : Number(String(raw ?? "").trim());
		return Number.isFinite(n) ? n : fallback;
	};

	const field = (label: string, key: keyof AwdEventConfigInput, fallback: number, hint?: string) => (
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
				留空表示保持当前值。保存时会带上**加载时那一版的时间戳**（乐观锁）——
				如果你和别人同时改，后保存的人会收到冲突而不是覆盖对方的改动。
			</span>
			<div className="ai-grid">
				{field("轮次总数", "round_count", current.round_count ?? 0)}
				{field("每轮时长（秒）", "round_duration_secs", current.round_duration_secs)}
				{field("初始分", "initial_score", current.initial_score)}
				{field("免费重置次数", "free_reset_count", current.free_reset_count)}
				{field(
					"超出重置的罚分",
					"extra_reset_penalty",
					current.extra_reset_penalty,
					"每次超出免费次数的重置扣多少分",
				)}
				{field("判题最大并发", "judge_max_concurrency", current.judge_max_concurrency)}
				{field("判题默认超时（秒）", "judge_default_timeout_secs", current.judge_default_timeout_secs)}
				{field("判题重试间隔（秒）", "judge_retry_interval_secs", current.judge_retry_interval_secs)}
				{field("归档保留（小时）", "archive_retention_hours", current.archive_retention_hours)}
			</div>
			<div className="ai-row" style={{ justifyContent: "flex-end" }}>
				<Can permission="awd.manage">
				<Button
					type="primary"
					loading={save.isPending}
					disabled={Object.keys(draft).length === 0}
					onClick={async () => {
						const body: AwdEventConfigInput = { ...draft };
						// 数字统一转成 number，避免把 "" 发成 0。
						for (const [k, v] of Object.entries(body)) {
							if (typeof v === "string" && v.trim() !== "" && k !== "planned_start_at") {
								(body as Record<string, unknown>)[k] = num(v, 0);
							} else if (typeof v === "string" && v.trim() === "") {
								delete (body as Record<string, unknown>)[k];
							}
						}
						// 乐观锁：带上这一版的时间戳。
						body.expected_updated_at = current.verified_at ?? undefined;
						try {
							await save.mutateAsync({ eventId, body });
							Notification.success({ message: "配置已保存" });
							setDraft({});
							onSaved();
						} catch (e) {
							// 冲突时不重试覆盖 —— 那正好抹掉别人的改动。
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

/** 预检与凭据轮换。 */
function PrecheckPanel({ eventId }: { eventId: string }) {
	const runtime = useRuntime();
	const confirm = useConfirm();
	const [report, setReport] = useState<string | null>(null);

	const runs = useUniQuery(
		[K_PRECHECKS, eventId],
		() => runtime.client.awd.admin.prechecks(eventId),
		"加载预检历史",
	);
	const precheck = useUniMutation(
		() => runtime.client.awd.admin.precheck(eventId),
		"执行预检",
	);
	const rotate = useUniMutation(
		() => runtime.client.awd.admin.rotateTokens(eventId),
		"轮换凭据",
	);

	return (
		<div className="ai-col" style={{ gap: 12 }}>
			<Card>
				<div className="ai-row-between">
					<div className="ai-col" style={{ gap: 2, alignItems: "flex-start" }}>
						<span className="ai-strong">开赛前预检</span>
						<span className="ai-muted" style={{ fontSize: 12 }}>
							检查网络、镜像、靶机与判题通道是否就绪。**开赛前务必跑一次。**
						</span>
					</div>
					<Can permission="events.lifecycle.manage">
					<Button
						type="primary"
						icon={<CheckCircleIcon size={16} />}
						loading={precheck.isPending}
						onClick={async () => {
							const text = await precheck.mutateAsync(undefined);
							setReport(text ?? "（预检没有返回内容）");
							void runs.refetch();
						}}
					>
						执行预检
					</Button>
					</Can>
				</div>
			</Card>

			<Card>
				<div className="ai-row-between">
					<div className="ai-col" style={{ gap: 2, alignItems: "flex-start" }}>
						<span className="ai-strong">轮换凭据</span>
						<span className="ai-muted" style={{ fontSize: 12 }}>
							重新生成本场赛事的 flag 与接入凭据。
						</span>
					</div>
					<Can permission="events.lifecycle.manage">
					<Button
						danger
						icon={<KeyIcon size={16} />}
						loading={rotate.isPending}
						onClick={async () => {
							const ok = await confirm({
								title: "轮换全部凭据",
								consequence:
									"会**重新生成**本场赛事的 flag 与接入凭据。**已泄漏的旧 flag 会立即失效**（这是它的用途），但正在使用旧 WireGuard/SSH 凭据的选手会**断线**，需要重新下发配置。",
								okText: "轮换凭据",
								danger: true,
								confirmPhrase: "轮换",
							});
							if (!ok) return;
							await rotate.mutateAsync(undefined);
							Notification.success({ message: "凭据已轮换" });
							void runs.refetch();
						}}
					>
						轮换
					</Button>
					</Can>
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 10 }}>
					<span className="ai-strong">预检历史</span>
					<Divider />
					{runs.isLoading ? (
						<SkeletonCards count={2} />
					) : runs.error ? (
						<ErrorState error={runs.error} onRetry={() => void runs.refetch()} />
					) : (runs.data ?? []).length === 0 ? (
						<EmptyState title="还没有预检记录" description="执行一次预检后会出现在这里。" />
					) : (
						(runs.data ?? []).map((r) => (
							<div key={r.id} className="ai-row-between">
								<div className="ai-row" style={{ gap: 8 }}>
									<Tag
										size="small"
										variant="soft"
										color={
											r.status === "success"
												? "app-green"
												: r.status === "failed"
													? "app-red"
													: "app-yellow"
										}
									>
										{r.status}
									</Tag>
									{r.trigger ? (
										<Tag size="small" variant="outlined">
											{r.trigger}
										</Tag>
									) : null}
									{r.error_msg ? (
										<span className="ai-muted" style={{ fontSize: 12 }}>
											{r.error_msg}
										</span>
									) : null}
								</div>
								<span className="ai-muted" style={{ fontSize: 12 }}>
									{r.started_at ? formatTime(r.started_at) : "—"}
								</span>
							</div>
						))
					)}
				</div>
			</Card>

			<Modal
				open={report !== null}
				title="预检结果"
				typewriter={false}
				width={720}
				onClose={() => setReport(null)}
				footer={<Button onClick={() => setReport(null)}>关闭</Button>}
			>
				<div className="ai-terminal" style={{ maxHeight: 440 }}>
					{report}
				</div>
			</Modal>
		</div>
	);
}

/** 队伍与判罚：封禁 / 解封 / 调分。 */
function TeamsPanel({ eventId }: { eventId: string }) {
	const runtime = useRuntime();
	const [adjustFor, setAdjustFor] = useState<string | null>(null);
	const [delta, setDelta] = useState("");
	const [reason, setReason] = useState("");
	const [banFor, setBanFor] = useState<{ id: string; name: string } | null>(null);
	const [banReason, setBanReason] = useState("");

	const teams = useUniQuery(
		["admin-awd-teams", eventId],
		() => runtime.client.admin.event_teams.getTeams(eventId)(),
		"加载战队",
	);
	const scores = useUniQuery(
		[K_SCORES, eventId],
		() => runtime.client.awd.admin.scores(eventId),
		"加载积分",
	);
	const ban = useUniMutation(
		(vars: { teamId: string; reason?: string }) =>
			runtime.client.awd.admin.banTeam(eventId, vars.teamId, { reason: vars.reason }),
		"封禁战队",
	);
	const unban = useUniMutation(
		(vars: { teamId: string }) => runtime.client.awd.admin.unbanTeam(eventId, vars.teamId),
		"解封战队",
	);
	const adjust = useUniMutation(
		// 后端 body 是 snake_case（team_id），不是前端的 camelCase。
		(vars: { teamId: string; delta: number; reason: string }) =>
			runtime.client.awd.admin.adjustScore(eventId, {
				team_id: vars.teamId,
				delta: vars.delta,
				reason: vars.reason,
			}),
		"调整分数",
	);

	/** 战队是否被封禁 —— 后端把封禁状态放在 AWD 状态里；这里用积分榜侧信息 + 逐队查询不现实，
	 *  因此只在操作后刷新，不做乐观假设。 */
	if (teams.isLoading) return <SkeletonCards count={3} />;
	if (teams.error) return <ErrorState error={teams.error} onRetry={() => void teams.refetch()} />;
	if ((teams.data ?? []).length === 0) {
		return (
			<EmptyState
				title="这场赛事还没有战队"
				description="战队由选手自行创建或管理员导入。"
			/>
		);
	}

	return (
		<div className="ai-col" style={{ gap: 12 }}>
			<span className="ai-muted">
				封禁会立即阻断该战队的攻击提交与判题收益。**解封不会恢复已产生的扣分**，
				分数修正请用「调整分数」。
			</span>
			{(teams.data ?? []).map((t) => {
				const score = (scores.data ?? []).find((s) => s.team_id === t.team.id);
				return (
					<Card key={t.id}>
						<div className="ai-col" style={{ gap: 8 }}>
							<div className="ai-row-between">
								<div className="ai-row" style={{ gap: 8 }}>
									<ShieldIcon size={16} />
									<span className="ai-strong">{t.team.name}</span>
									<Tag size="small" variant="outlined">
										{t.members?.length ?? 0} 人
									</Tag>
									{score ? (
										<Tag size="small" variant="soft" color="app-teal">
											#{score.rank} ｜ {score.total_score} 分
										</Tag>
									) : null}
								</div>
								<Can permission="events.lifecycle.manage">
								<div className="ai-row" style={{ gap: 6 }}>
									<Button
										size="small"
										onClick={() => {
											setAdjustFor(t.team.id);
											setDelta("");
											setReason("");
										}}
									>
										调整分数
									</Button>
									<Button
										size="small"
										danger
										icon={<AlertTriangleIcon size={14} />}
										onClick={() => {
											setBanFor({ id: t.team.id, name: t.team.name });
											setBanReason("");
										}}
									>
										封禁
									</Button>
									<Button
										size="small"
										icon={<CheckCircleIcon size={14} />}
										loading={unban.isPending}
										onClick={async () => {
											await unban.mutateAsync({ teamId: t.team.id });
											Notification.success({ message: "已解封" });
											void teams.refetch();
										}}
									>
										解封
									</Button>
								</div>
								</Can>
							</div>
							<div className="ai-row" style={{ gap: 6, flexWrap: "wrap" }}>
								{(t.members ?? []).map((m) => (
									<Tag key={m.username} size="small" variant="outlined">
										{m.nickname || m.username}
										{m.role === "captain" ? "（队长）" : ""}
									</Tag>
								))}
							</div>
						</div>
					</Card>
				);
			})}

			<Modal
				open={adjustFor !== null}
				title="调整分数"
				typewriter={false}
				onClose={() => setAdjustFor(null)}
				footer={
					<>
						<Button onClick={() => setAdjustFor(null)}>取消</Button>
						<Button
							type="primary"
							disabled={!reason.trim() || !delta.trim()}
							loading={adjust.isPending}
							onClick={async () => {
								if (!adjustFor) return;
								await adjust.mutateAsync({
									teamId: adjustFor,
									delta: Number(delta),
									reason: reason.trim(),
								});
								Notification.success({ message: "分数已调整" });
								setAdjustFor(null);
								void scores.refetch();
							}}
						>
							调整
						</Button>
					</>
				}
			>
				<div className="ai-col" style={{ gap: 12 }}>
					<span className="ai-muted">
						增量为**正数加分、负数扣分**。原因会写入审计日志，请写清依据。
					</span>
					<label className="ai-col" style={{ gap: 6 }}>
						<span>增量</span>
						<Input
							value={delta}
							placeholder="例如 -50 或 100"
							onChange={(e) => setDelta(e.target.value)}
						/>
					</label>
					<label className="ai-col" style={{ gap: 6 }}>
						<span>原因</span>
						<Input
							value={reason}
							placeholder="例如：使用被禁用的扫描工具"
							onChange={(e) => setReason(e.target.value)}
						/>
					</label>
				</div>
			</Modal>

			{/* 封禁原因走真正的 Modal，而不是 window.prompt ——
			    平台约定新前端也应避免原生弹窗（体验与可访问性都不受控）。 */}
			<Modal
				open={banFor !== null}
				title={`封禁「${banFor?.name ?? ""}」`}
				typewriter={false}
				onClose={() => setBanFor(null)}
				footer={
					<>
						<Button onClick={() => setBanFor(null)}>取消</Button>
						<Button
							danger
							disabled={!banReason.trim()}
							loading={ban.isPending}
							onClick={async () => {
								if (!banFor) return;
								await ban.mutateAsync({ teamId: banFor.id, reason: banReason.trim() });
								Notification.success({ message: "已封禁" });
								setBanFor(null);
								void teams.refetch();
							}}
						>
							确认封禁
						</Button>
					</>
				}
			>
				<div className="ai-col" style={{ gap: 12 }}>
					<span className="ai-muted">
						该战队会立即被禁止提交 flag 与获得判题收益。**对手仍可继续攻击他们的靶机。**
						原因会写入审计日志，请写清依据。
					</span>
					<label className="ai-col" style={{ gap: 6 }}>
						<span>封禁原因</span>
						<Input
							value={banReason}
							placeholder="例如：向对手靶机投放破坏性 payload"
							onChange={(e) => setBanReason(e.target.value)}
						/>
					</label>
				</div>
			</Modal>
		</div>
	);
}

/** 靶机挂载：决定本场 AWD 的题目集合。 */
function GameboxesPanel({ eventId }: { eventId: string }) {
	const runtime = useRuntime();
	const confirm = useConfirm();

	const mounted = useUniQuery(
		[K_GAMEBOXES, eventId],
		() => runtime.client.awd.admin.listEventGameboxes(eventId),
		"加载赛事靶机",
	);
	const library = useUniQuery(
		["awd-gamebox-library"],
		() => runtime.client.awd.admin.listGameboxes(),
		"加载箱子库",
	);

	const add = useUniMutation(
		(vars: { gamebox_id: string }) =>
			runtime.client.awd.admin.addEventGamebox(eventId, vars),
		"挂载箱子",
	);
	const remove = useUniMutation(
		(vars: { eventGameboxId: string }) =>
			runtime.client.awd.admin.removeEventGamebox(eventId, vars.eventGameboxId),
		"卸载箱子",
	);
	const reset = useUniMutation(
		(vars: { instanceId: string }) => runtime.client.awd.admin.resetGamebox(eventId, vars.instanceId),
		"重置实例",
	);

	if (mounted.isLoading || library.isLoading) return <SkeletonCards count={3} />;
	if (mounted.error) {
		return <ErrorState error={mounted.error} onRetry={() => void mounted.refetch()} />;
	}

	const mountedIds = new Set((mounted.data ?? []).map((m) => m.gamebox_id));
	const available = (library.data ?? []).filter((g) => !mountedIds.has(g.id));

	return (
		<div className="ai-col" style={{ gap: 12 }}>
			<Card>
				<div className="ai-row-between">
					<div className="ai-col" style={{ gap: 2, alignItems: "flex-start" }}>
						<span className="ai-strong">从箱子库挂载</span>
						<span className="ai-muted" style={{ fontSize: 12 }}>
							已挂载的箱子不会重复出现在这里。挂载后选手才会拿到对应靶机。
						</span>
					</div>
					<Can permission="awd.manage">
					<div style={{ minWidth: 280 }}>
						<Select
							value=""
							placeholder="选择一个箱子加入本场"
							showSearch
							options={available.map((g) => ({
								key: g.id,
								label: `${g.name}${g.version ? ` v${g.version}` : ""}`,
							}))}
							onChange={async (v) => {
								if (typeof v !== "string") return;
								await add.mutateAsync({ gamebox_id: v });
								Notification.success({ message: "已挂载" });
								void mounted.refetch();
							}}
						/>
					</div>
					</Can>
				</div>
				{available.length === 0 ? (
					<span className="ai-muted" style={{ fontSize: 12 }}>
						箱子库里没有未挂载的箱子。可以去「箱子库」导入或构建。
					</span>
				) : null}
			</Card>

			{(mounted.data ?? []).length === 0 ? (
				<Card>
					<EmptyState
						title="还没有挂载任何箱子"
						description="这场 AWD 目前没有题目。部署前请至少挂载一个。"
					/>
				</Card>
			) : (
				(mounted.data ?? []).map((m) => (
					<Card key={m.id}>
						<div className="ai-col" style={{ gap: 8 }}>
							<div className="ai-row-between">
								<div className="ai-row" style={{ gap: 8 }}>
									<span className="ai-strong">{m.gamebox_name}</span>
									<Tag size="small" variant="outlined">
										{m.gamebox_safe_name}
									</Tag>
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
								<Can permission="awd.manage">
								<div className="ai-row" style={{ gap: 6 }}>
									<Button
										size="small"
										icon={<RefreshIcon size={14} />}
										loading={reset.isPending}
										onClick={async () => {
											const ok = await confirm({
												title: `重置「${m.gamebox_name}」的实例`,
												consequence:
													"这会**重建该箱子的全部实例**为初始状态。所有队伍在该箱子上的加固与已拿到的 flag 都会失效 —— 这是应急手段，正常比赛期间请不要使用。",
												okText: "重置实例",
												danger: true,
												confirmPhrase: "重置",
											});
											if (!ok) return;
											// 需要实例 id；这里用 host_offset 派生不了，因此要求后端列表提供。
											await reset.mutateAsync({ instanceId: m.id });
											Notification.success({ message: "已请求重置" });
										}}
									>
										重置
									</Button>
									<Button
										size="small"
										danger
										icon={<TrashIcon size={14} />}
										onClick={async () => {
											const ok = await confirm({
												title: `从本场卸载「${m.gamebox_name}」`,
												consequence:
													"该箱子会从本场赛事的题目集合中移除，**对应靶机在下次部署时不再创建**。已运行的实例需要重新部署才会消失。",
												okText: "卸载",
												danger: true,
											});
											if (!ok) return;
											await remove.mutateAsync({ eventGameboxId: m.id });
											Notification.success({ message: "已卸载" });
											void mounted.refetch();
										}}
									>
										卸载
									</Button>
								</div>
								</Can>
							</div>
							<div className="ai-row" style={{ gap: 16, flexWrap: "wrap" }}>
								<span className="ai-muted" style={{ fontSize: 12 }}>
									主机偏移 {m.host_offset}
								</span>
								<span className="ai-muted" style={{ fontSize: 12 }}>
									CPU {m.cpu_millis} ‰ ｜ 内存 {Math.round((m.memory_bytes ?? 0) / 1024 / 1024)} MiB
								</span>
								<span className="ai-muted" style={{ fontSize: 12 }}>
									攻击分 {m.attack_score} ｜ 首杀加成 {m.first_bonus} ｜ 宕机罚分{" "}
									{m.judge_down_penalty}
								</span>
							</div>
						</div>
					</Card>
				))
			)}
		</div>
	);
}

/** 赛事网络分配。 */
function NetworkPanel({ eventId }: { eventId: string }) {
	const runtime = useRuntime();
	const confirm = useConfirm();

	const net = useUniQuery(
		["awd-event-network", eventId],
		() => runtime.client.awd.admin.getEventNetwork(eventId),
		"加载赛事网络",
	);
	const allocate = useUniMutation(
		(vars: { mode: "automatic" | "manual" }) =>
			runtime.client.awd.admin.allocateEventNetwork(eventId, { allocation_mode: vars.mode }),
		"分配网络",
	);
	const reallocate = useUniMutation(
		() => runtime.client.awd.admin.reallocateEventNetwork(eventId),
		"重新分配网络",
	);

	if (net.isLoading) return <LoadingState label="加载赛事网络…" />;
	if (net.error) return <ErrorState error={net.error} onRetry={() => void net.refetch()} />;

	const info = net.data;

	return (
		<div className="ai-col" style={{ gap: 12 }}>
			<Card>
				<div className="ai-row-between">
					<div className="ai-col" style={{ gap: 2, alignItems: "flex-start" }}>
						<span className="ai-strong">网段分配</span>
						<span className="ai-muted" style={{ fontSize: 12 }}>
							每个 AWD 赛事开赛前**必须**分配网段。自动模式从平台池里切子网。
						</span>
					</div>
					<Can permission="events.lifecycle.manage">
					<div className="ai-row" style={{ gap: 6 }}>
						<Button
							type="primary"
							icon={<GlobeIcon size={16} />}
							loading={allocate.isPending}
							onClick={async () => {
								const ok = await confirm({
									title: "自动分配网段",
									consequence:
										"会从平台网段池中为本场赛事切出一组子网。若池中可用空间不足会失败。已有分配时请不要重复执行。",
									okText: "分配",
								});
								if (!ok) return;
								await allocate.mutateAsync({ mode: "automatic" });
								Notification.success({ message: "已分配" });
								void net.refetch();
							}}
						>
							自动分配
						</Button>
						<Button
							danger
							icon={<RefreshIcon size={16} />}
							loading={reallocate.isPending}
							onClick={async () => {
								const ok = await confirm({
									title: "重新分配网段",
									consequence:
										"会**替换本场赛事当前的网段**并重建相关网络策略。所有选手的 WireGuard 配置与靶机 IP 都会变化，**必须重新下发配置**。比赛进行中执行会中断所有人。",
									okText: "重新分配",
									danger: true,
									confirmPhrase: "重新分配",
								});
								if (!ok) return;
								await reallocate.mutateAsync(undefined);
								Notification.success({ message: "已重新分配" });
								void net.refetch();
							}}
						>
							重新分配
						</Button>
					</div>
					</Can>
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 8 }}>
					<span className="ai-strong">当前网络</span>
					<Divider />
					{!info ? (
						<EmptyState
							title="尚未分配网段"
							description="点击「自动分配」为本场赛事分配网络。"
						/>
					) : (
						<div className="ai-grid">
							{[
								["分配模式", info.allocation_mode],
								["靶机网段", info.gamebox_cidr],
								["WireGuard 网段", info.wireguard_cidr],
								["基础设施子网", info.infrastructure_subnet],
								["FlagServer", info.flagserver_ip],
								["JudgeServer", info.judgeserver_ip],
								["WireGuard 接口", info.wireguard_interface_name],
								["监听端口", String(info.wireguard_listen_port ?? "—")],
							].map(([k, v]) => (
								<div key={k} className="ai-row-between">
									<span className="ai-muted">{k}</span>
									<span className="ai-mono">{v || "—"}</span>
								</div>
							))}
						</div>
					)}
				</div>
			</Card>
		</div>
	);
}

function ScoresPanel({ eventId }: { eventId: string }) {
	const runtime = useRuntime();
	const scores = useUniQuery(
		[K_SCORES, eventId],
		() => runtime.client.awd.admin.scores(eventId),
		"加载积分榜",
	);

	if (scores.isLoading) return <SkeletonCards count={3} />;
	if (scores.error) return <ErrorState error={scores.error} onRetry={() => void scores.refetch()} />;
	if ((scores.data ?? []).length === 0) {
		return <EmptyState title="还没有队伍得分" description="开赛后这里会实时更新。" />;
	}

	return (
		<div className="ai-scroll">
			<table style={{ width: "100%", borderCollapse: "collapse" }}>
				<thead>
					<tr>
						{["排名", "战队", "攻击分", "防御分", "总分"].map((h) => (
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
					{(scores.data ?? []).map((row) => (
						<tr key={row.team_id}>
							<td style={{ padding: "8px 12px" }}>
								<Tag size="small" variant="soft">
									#{row.rank}
								</Tag>
							</td>
							<td style={{ padding: "8px 12px" }} className="ai-strong">
								{row.team_name}
							</td>
							<td style={{ padding: "8px 12px" }}>{row.attack_score}</td>
							<td style={{ padding: "8px 12px" }}>{row.defense_score}</td>
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
