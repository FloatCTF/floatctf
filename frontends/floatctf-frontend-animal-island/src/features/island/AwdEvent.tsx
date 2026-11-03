/**
 * AWD 选手端控制台。
 *
 * ## query key 是对齐过的，这不是巧合
 *
 * 这里用的 key（`awd-player-status` / `awd-gameboxes` / `awd-scores` / `awd-wg` /
 * `awd-ssh`，一律 `[key, eventId]`）与 `@floatctf/react` 的
 * `AWD_PLAYER_QUERY_KEYS` **逐字一致**。原因：SSE 连上后轮询回退会**停止**
 * （两者互斥）。失效列表一旦比页面实际读的 key 窄，面板就会在整场比赛里
 * 悄悄停止更新 —— 而连接状态看起来完全正常。所以这里不自己发明 key 名。
 *
 * ## 实时状态必须可见
 *
 * `connected` 为 false 时页面**显式**告诉用户"正在轮询兜底"，
 * 而不是让用户以为看到的是实时的。比赛里的分数与封禁状态不能靠猜。
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
} from "animal-island-ui";
import {
	AlertTriangleIcon,
	FlagIcon,
	GlobeIcon,
	KeyIcon,
	LinkIcon,
	RefreshIcon,
	ShieldIcon,
	CodeIcon,
	WifiIcon,
} from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { EmptyState, ErrorState, LoadingState, SkeletonCards } from "~/components/states";
import { StatCard } from "~/components/resource";
import { createBindings } from "~/api/react";

/** 与 `AWD_PLAYER_QUERY_KEYS` 对齐的本地 key 常量（改这里 = 同时改失效列表）。 */
const K_STATUS = "awd-player-status";
const K_GAMEBOXES = "awd-gameboxes";
const K_SCORES = "awd-scores";
const K_WG = "awd-wg";
const K_SSH = "awd-ssh";

export function AwdEvent() {
	const { eventId = "" } = useParams();
	const runtime = useRuntime();
	const navigate = useNavigate();

	// 绑定按客户端创建；useMemo 保证同一客户端不会反复重建 hook 工厂。
	const bindings = useState(() => createBindings(runtime.client))[0];
	const stream = bindings.useAwdEventStream({ eventId, enabled: Boolean(eventId) });

	const status = useUniQuery(
		[K_STATUS, eventId],
		() => runtime.client.awd.player.status(eventId),
		"加载战况",
		{ enabled: Boolean(eventId) },
	);
	const gameboxes = useUniQuery(
		[K_GAMEBOXES, eventId],
		() => runtime.client.awd.player.gameboxes(eventId),
		"加载靶机",
		{ enabled: Boolean(eventId) },
	);
	const scores = useUniQuery(
		[K_SCORES, eventId],
		() => runtime.client.awd.player.scores(eventId),
		"加载积分榜",
		{ enabled: Boolean(eventId) },
	);

	if (!eventId) {
		return (
			<Card>
				<EmptyState
					title="没有指定赛事"
					description="AWD 面板必须绑定到一场赛事。"
					action={<Button onClick={() => void navigate("/island/events")}>去赛事列表</Button>}
				/>
			</Card>
		);
	}

	if (status.isLoading) return <LoadingState label="加载 AWD 战况…" />;
	if (status.error) {
		return <ErrorState error={status.error} onRetry={() => void status.refetch()} />;
	}
	if (!status.data) {
		return (
			<ErrorState
				error={new Error("后端没有返回 AWD 状态")}
				onRetry={() => void status.refetch()}
			/>
		);
	}

	const st = status.data;
	const myBoxes = gameboxes.data ?? [];
	const healthy = myBoxes.filter((b) => /up|healthy|ok/i.test(b.health_status ?? "")).length;

	return (
		<>
			<PageHeader
				title="AWD 攻防面板"
				description={`阶段 ${st.phase}｜状态 ${st.status}`}
				extra={
					<>
						{/* 实时状态是可见的：断了就说断了，不假装实时。 */}
						<Tooltip
							title={
								stream.connected
									? "事件流已连接，分数与轮次会自动刷新"
									: "事件流不可用，正在用轮询兜底（约每 15 秒一次）"
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
							onClick={() => {
								void status.refetch();
								void gameboxes.refetch();
								void scores.refetch();
							}}
						>
							刷新
						</Button>
					</>
				}
			/>

			{st.banned ? (
				<Card>
					<div className="ai-row" style={{ gap: 8 }}>
						<AlertTriangleIcon size={18} />
						<span className="ai-strong">
							你（或你的战队）已被封禁。攻击与提交会被后端拒绝，请联系赛事管理员。
						</span>
					</div>
				</Card>
			) : null}

			{stream.lastError && !stream.connected ? (
				<Card>
					<div className="ai-row" style={{ gap: 8 }}>
						<AlertTriangleIcon size={16} />
						<span className="ai-muted">
							事件流错误：{stream.lastError.message}（页面仍在轮询兜底，数据不会停更）
						</span>
					</div>
				</Card>
			) : null}

			<div className="ai-grid ai-grid-wide">
				<StatCard
					label="当前轮次"
					value={st.current_round ?? "—"}
					hint={st.round_count != null ? `共 ${st.round_count} 轮` : undefined}
				/>
				<StatCard label="我的总分" value={st.score ?? "—"} icon={<FlagIcon size={18} />} />
				<StatCard
					label="我的靶机"
					value={myBoxes.length}
					hint={`健康 ${healthy}`}
					icon={<ShieldIcon size={18} />}
				/>
				<StatCard
					label="终局结算"
					value={st.final_settlement ? "已结束" : "进行中"}
					hint={st.final_settlement ? "最后一轮已完成，判题仍在收尾" : undefined}
				/>
			</div>

			<Card>
				<Tabs
					aria-label="AWD 分区"
					defaultActiveKey="attack"
					items={[
						{
							key: "attack",
							label: "提交 flag",
							children: <AttackPanel eventId={eventId} onDone={() => void scores.refetch()} />,
						},
						{
							key: "boxes",
							label: "我的靶机",
							children: <GameboxPanel eventId={eventId} boxes={myBoxes} query={gameboxes} />,
						},
						{
							key: "scores",
							label: "积分榜",
							children: <ScorePanel scores={scores} />,
						},
						{
							key: "access",
							label: "接入方式",
							children: <AccessPanel eventId={eventId} />,
						},
					]}
				/>
			</Card>
		</>
	);
}

/** flag 提交：AWD 的得分核心动作。 */
function AttackPanel({ eventId, onDone }: { eventId: string; onDone: () => void }) {
	const runtime = useRuntime();
	const [flag, setFlag] = useState("");
	const submit = useUniMutation(
		(vars: { id: string; flag: string }) => runtime.client.awd.player.submitFlag(vars.id, vars.flag),
		"提交 flag",
	);

	async function doSubmit() {
		const value = flag.trim();
		if (!value) return;
		try {
			await submit.mutateAsync({ id: eventId, flag: value });
			Notification.success({ message: "flag 已提交", description: "若有效会立即计入攻击分。" });
			setFlag("");
			onDone();
		} catch {
			// 失败已由统一的 mutation 错误提示呈现；保留输入让用户改。
		}
	}

	return (
		<div className="ai-col" style={{ gap: 12 }}>
			<span className="ai-muted">
				AWD 的 flag 来自**其它战队**的靶机。提交自己队伍的 flag 不会得分。
			</span>
			<div className="ai-row" style={{ gap: 8 }}>
				<div style={{ flex: 1, minWidth: 260 }}>
					<Input
						allowClear
						placeholder="flag{...}"
						prefix={<FlagIcon size={16} />}
						value={flag}
						onChange={(e) => setFlag(e.target.value)}
						onKeyDown={(e) => {
							if (e.key === "Enter") void doSubmit();
						}}
					/>
				</div>
				<Button
					type="primary"
					icon={<FlagIcon size={16} />}
					loading={submit.isPending}
					disabled={!flag.trim()}
					onClick={() => void doSubmit()}
				>
					提交
				</Button>
			</div>
		</div>
	);
}

/** 我的靶机：IP / 健康 / 重置（受免费次数与罚分约束）。 */
function GameboxPanel({
	eventId,
	boxes,
	query,
}: {
	eventId: string;
	boxes: Array<{
		id: string;
		gamebox_name: string;
		status: string;
		gamebox_ip: string;
		container_name: string;
		health_status: string;
	}>;
	query: { isLoading: boolean; error: unknown; refetch: () => void };
}) {
	const runtime = useRuntime();
	const confirm = useConfirm();
	const reset = useUniMutation(
		(vars: { eventId: string; instanceId: string }) =>
			runtime.client.awd.player.resetGamebox(vars.eventId, vars.instanceId),
		"重置靶机",
	);

	if (query.isLoading) return <SkeletonCards count={2} />;
	if (query.error) {
		return (
			<ErrorState
				error={query.error}
				onRetry={() => query.refetch()}
			/>
		);
	}
	if (boxes.length === 0) {
		return (
			<EmptyState
				title="你还没有靶机"
				description="靶机由赛事管理员在开赛后分配。如果没有，请联系管理员。"
			/>
		);
	}

	return (
		<div className="ai-col" style={{ gap: 12 }}>
			{boxes.map((b) => (
				<Card key={b.id}>
					<div className="ai-col" style={{ gap: 8 }}>
						<div className="ai-row-between">
							<span className="ai-strong">{b.gamebox_name}</span>
							<div className="ai-row" style={{ gap: 8 }}>
								<Tag size="small" variant="outlined">
									{b.status}
								</Tag>
								<Tag
									size="small"
									variant="soft"
									color={/up|healthy|ok/i.test(b.health_status ?? "") ? "app-green" : "app-red"}
								>
									健康 {b.health_status || "未知"}
								</Tag>
							</div>
						</div>
						<div className="ai-row" style={{ gap: 16, flexWrap: "wrap" }}>
							<span className="ai-row" style={{ gap: 6 }}>
								<GlobeIcon size={14} />
								<span className="ai-mono">{b.gamebox_ip || "—"}</span>
							</span>
							<span className="ai-row" style={{ gap: 6 }}>
								<CodeIcon size={14} />
								<span className="ai-mono ai-muted">{b.container_name || "—"}</span>
							</span>
						</div>
						<div className="ai-row" style={{ justifyContent: "flex-end" }}>
							<Button
								size="small"
								danger
								icon={<RefreshIcon size={14} />}
								loading={reset.isPending}
								onClick={async () => {
									const ok = await confirm({
										title: `重置靶机「${b.gamebox_name}」`,
										consequence:
											"靶机会被**重建为初始状态**：你在里面做的所有修改（含防御加固）都会丢失，其它队伍已拿到的 flag 也会随之失效。**重置受免费次数限制，超出部分会扣分。**",
										okText: "重置靶机",
										danger: true,
										confirmPhrase: "重置",
									});
									if (!ok) return;
									await reset.mutateAsync({ eventId, instanceId: b.id });
									Notification.success({ message: "已请求重置" });
									query.refetch();
								}}
							>
								重置
							</Button>
						</div>
					</div>
				</Card>
			))}
		</div>
	);
}

function ScorePanel({
	scores,
}: {
	scores: { data?: Array<{ team_id: string; team_name: string; attack_score: number; defense_score: number; total_score: number; rank: number }>; isLoading: boolean; error: unknown; refetch: () => void };
}) {
	if (scores.isLoading) return <SkeletonCards count={3} />;
	if (scores.error) return <ErrorState error={scores.error} onRetry={() => scores.refetch()} />;
	if ((scores.data ?? []).length === 0) {
		return <EmptyState title="还没有队伍得分" description="比赛开始后这里会实时更新。" />;
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

/** 接入方式：WireGuard 配置与 SSH 凭据。 */
function AccessPanel({ eventId }: { eventId: string }) {
	const runtime = useRuntime();
	const [showSsh, setShowSsh] = useState(false);

	const wg = useUniQuery(
		[K_WG, eventId],
		() => runtime.client.awd.player.wireguardConfig(eventId),
		"加载 WireGuard 配置",
		{ enabled: Boolean(eventId) },
	);
	const ssh = useUniQuery(
		[K_SSH, eventId],
		() => runtime.client.awd.player.sshConfig(eventId),
		"加载 SSH 凭据",
		{ enabled: Boolean(eventId) && showSsh },
	);

	return (
		<div className="ai-col" style={{ gap: 12 }}>
			<Card>
				<div className="ai-col" style={{ gap: 10 }}>
					<div className="ai-row" style={{ gap: 8 }}>
						<LinkIcon size={18} />
						<span className="ai-strong">WireGuard 配置</span>
						<span className="ai-muted">这是接入 AWD 网络的唯一方式。</span>
					</div>
					<Divider />
					{wg.isLoading ? (
						<Loading active tip="加载配置…" />
					) : wg.error ? (
						<ErrorState error={wg.error} onRetry={() => void wg.refetch()} />
					) : wg.data?.config ? (
						<>
							<div className="ai-terminal" style={{ maxHeight: 300 }}>
								{wg.data.config}
							</div>
							<Button
								size="small"
								onClick={() => {
									void navigator.clipboard.writeText(wg.data?.config ?? "");
									Notification.success({ message: "配置已复制到剪贴板" });
								}}
							>
								复制配置
							</Button>
						</>
					) : (
						<EmptyState
							title="没有可用的 WireGuard 配置"
							description="比赛尚未开始，或你的战队还没有分配网络。"
						/>
					)}
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 10 }}>
					<div className="ai-row-between">
						<div className="ai-row" style={{ gap: 8 }}>
							<KeyIcon size={18} />
							<span className="ai-strong">SSH 凭据</span>
						</div>
						<Button
							size="small"
							icon={<RefreshIcon size={14} />}
							loading={ssh.isLoading}
							onClick={() => {
								setShowSsh(true);
								void ssh.refetch();
							}}
						>
							读取凭据
						</Button>
					</div>
					<Divider />
					{!showSsh ? (
						<span className="ai-muted">
							SSH 密码**不会自动加载** —— 点击「读取凭据」才会从后端取一次。
							这样密码不会在你只是路过这一页时也出现在屏幕上。
						</span>
					) : ssh.error ? (
						<ErrorState error={ssh.error} onRetry={() => void ssh.refetch()} />
					) : ssh.data ? (
						<div className="ai-col" style={{ gap: 8 }}>
							<div className="ai-row" style={{ gap: 12 }}>
								<Tag size="small" variant="outlined">
									端口 {ssh.data.port}
								</Tag>
								<Tag size="small" variant="soft" color="app-yellow">
									密码 {ssh.data.password}
								</Tag>
							</div>
							{(ssh.data.instances ?? []).length > 0 ? (
								<div className="ai-col" style={{ gap: 4 }}>
									<span className="ai-strong">可连实例</span>
									{ssh.data.instances.map((i) => (
										<div key={i.id} className="ai-row-between">
											<div className="ai-col" style={{ gap: 2, alignItems: "flex-start" }}>
												<span className="ai-mono ai-strong">{i.gamebox_ip}</span>
												<span className="ai-muted" style={{ fontSize: 12 }}>
													{i.container_name} ｜ 用户 {i.username}
												</span>
											</div>
											<Tag
												size="small"
												variant="soft"
												color={/up|healthy|ok/i.test(i.health_status ?? "") ? "app-green" : "app-red"}
											>
												{i.health_status || "未知"}
											</Tag>
										</div>
									))}
								</div>
							) : null}
						</div>
					) : null}
				</div>
			</Card>
		</div>
	);
}
