/**
 * 靶场网络（AWD 平台级设置）。
 *
 * 这是**宿主级**配置：改错会让新赛事拿不到网段，甚至影响正在运行的靶场。
 * 因此：
 * - 表单是"显式编辑"模式（默认只读，点「编辑」才可改），避免误触；
 * - 保存前必须输入确认词 —— 这不是流程繁琐，而是这类改动出错后
 *   需要人工上宿主收拾；
 * - 健康检查与已分配网段放在同一页：判断"能不能再开一场"要看这两个。
 */
import { useEffect, useState } from "react";
import {
	Button,
	Card,
	Divider,
	Input,
	Notification,
	Tag,
} from "animal-island-ui";
import {
	AlertTriangleIcon,
	CheckCircleIcon,
	EditIcon,
	GlobeIcon,
	RefreshIcon,
	SaveIcon,
} from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { Can } from "~/app/permissions";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { EmptyState, ErrorState, LoadingState } from "~/components/states";
import { StatCard } from "~/components/resource";
import { formatTime } from "~/lib/event";
import type { PlatformNetworkSettingsUpdate } from "@floatctf/sdk";

/** 健康项：值只有 "OK" 之类的字符串，因此按文本着色而不是猜枚举。 */
function healthTag(value: string | null | undefined) {
	if (value === null || value === undefined) {
		return (
			<Tag size="small" variant="outlined">
				未知
			</Tag>
		);
	}
	const ok = /^(ok|yes|true|up|enabled|active|on)$/i.test(value.trim());
	return (
		<Tag size="small" variant="soft" color={ok ? "app-green" : "app-red"}>
			{value}
		</Tag>
	);
}

export function ConsoleNetwork() {
	const runtime = useRuntime();
	const confirm = useConfirm();
	const [editing, setEditing] = useState(false);
	const [draft, setDraft] = useState<PlatformNetworkSettingsUpdate>({});

	const settings = useUniQuery(
		["console", "network"],
		() => runtime.client.awd.admin.getPlatformNetwork(),
		"加载网络设置",
	);
	const health = useUniQuery(
		["console", "network", "health"],
		() => runtime.client.awd.admin.getPlatformNetworkHealth(),
		"加载网络健康",
	);
	const allocations = useUniQuery(
		["console", "network", "allocations"],
		() => runtime.client.awd.admin.getPlatformNetworkAllocations(),
		"加载网段分配",
	);

	// 进入编辑态时用服务端当前值填充草稿；退出编辑态则丢弃草稿。
	useEffect(() => {
		if (settings.data) setDraft({ ...settings.data });
	}, [settings.data]);

	const save = useUniMutation(
		(vars: PlatformNetworkSettingsUpdate) => runtime.client.awd.admin.updatePlatformNetwork(vars),
		"保存网络设置",
	);

	if (settings.isLoading) return <LoadingState label="加载网络设置…" />;
	if (settings.error) {
		return <ErrorState error={settings.error} onRetry={() => void settings.refetch()} />;
	}
	if (!settings.data) {
		return (
			<ErrorState
				error={new Error("后端没有返回网络设置")}
				onRetry={() => void settings.refetch()}
			/>
		);
	}

	const s = settings.data;
	const active = (allocations.data ?? []).filter((a) => a.active).length;

	/** 数字字段统一走这个：避免把空串当成 0 提交上去。 */
	const num = (v: unknown, fallback: number): number => {
		const n = typeof v === "number" ? v : Number(String(v ?? "").trim());
		return Number.isFinite(n) ? n : fallback;
	};

	const field = (
		label: string,
		key: keyof PlatformNetworkSettingsUpdate,
		hint?: string,
	) => (
		<label className="ai-col" style={{ gap: 6 }}>
			<span>{label}</span>
			<Input
				disabled={!editing}
				value={String(draft[key] ?? "")}
				placeholder={String(s[key as keyof typeof s] ?? "")}
				onChange={(e) => setDraft({ ...draft, [key]: e.target.value })}
			/>
			{hint ? (
				<span className="ai-muted" style={{ fontSize: 12 }}>
					{hint}
				</span>
			) : null}
		</label>
	);

	return (
		<>
			<PageHeader
				title="靶场网络"
				description="AWD 的平台级网段与 WireGuard 设置。改动会影响之后创建的所有赛事。"
				extra={
					<>
						<Button
							size="small"
							icon={<RefreshIcon size={16} />}
							onClick={() => {
								void settings.refetch();
								void health.refetch();
								void allocations.refetch();
							}}
						>
							刷新
						</Button>
						{editing ? (
							<>
								<Button
									size="small"
									onClick={() => {
										setEditing(false);
										if (settings.data) setDraft({ ...settings.data });
									}}
								>
									取消
								</Button>
								<Button
									size="small"
									type="primary"
									icon={<SaveIcon size={16} />}
									loading={save.isPending}
									onClick={async () => {
										const ok = await confirm({
											title: "保存靶场网络设置",
											consequence:
												"这会修改**平台级**网段与端口分配。之后创建的 AWD 赛事会按新设置分配网段；**已经分配的赛事不受影响**，但若新网段与既有分配重叠，新赛事可能起不来。请确认已核对现有分配。",
											okText: "保存设置",
											danger: true,
											confirmPhrase: "保存",
										});
										if (!ok) return;
										await save.mutateAsync({
											...draft,
											gamebox_event_prefix: num(draft.gamebox_event_prefix, s.gamebox_event_prefix),
											gamebox_team_prefix: num(draft.gamebox_team_prefix, s.gamebox_team_prefix),
											wireguard_event_prefix: num(draft.wireguard_event_prefix, s.wireguard_event_prefix),
											wireguard_team_prefix: num(draft.wireguard_team_prefix, s.wireguard_team_prefix),
											wireguard_port_min: num(draft.wireguard_port_min, s.wireguard_port_min),
											wireguard_port_max: num(draft.wireguard_port_max, s.wireguard_port_max),
										});
										Notification.success({ message: "网络设置已保存" });
										setEditing(false);
										void settings.refetch();
									}}
								>
									保存
								</Button>
							</>
						) : (
							<Can permission="awd.manage">
								<Button
									size="small"
									type="primary"
									icon={<EditIcon size={16} />}
									onClick={() => setEditing(true)}
								>
									编辑
								</Button>
							</Can>
						)}
					</>
				}
			/>

			{editing ? (
				<Card>
					<div className="ai-row" style={{ gap: 8 }}>
						<AlertTriangleIcon size={16} />
						<span className="ai-muted">
							你正在编辑**宿主级**网络设置。保存需要输入确认词。
						</span>
					</div>
				</Card>
			) : null}

			<div className="ai-grid ai-grid-wide">
				<StatCard
					label="使用中的网段"
					value={active}
					icon={<GlobeIcon size={18} />}
					hint="来自网段分配记录"
				/>
				<StatCard
					label="已分配总数"
					value={allocations.data?.length ?? 0}
					hint="含已释放"
				/>
				<StatCard
					label="最后更新"
					value={<span style={{ fontSize: 14 }}>{formatTime(s.updated_at)}</span>}
				/>
			</div>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<span className="ai-strong">网段池</span>
					<Divider />
					<div className="ai-grid">
						{field("Gamebox 网段池", "gamebox_pool", "形如 10.43.0.0/16；每个赛事从中切一个子网")}
						{field("Gamebox 赛事位偏移", "gamebox_event_prefix")}
						{field("Gamebox 队伍位偏移", "gamebox_team_prefix")}
						{field("WireGuard 网段池", "wireguard_pool")}
						{field("WireGuard 赛事位偏移", "wireguard_event_prefix")}
						{field("WireGuard 队伍位偏移", "wireguard_team_prefix")}
						{field("WireGuard 端口下界", "wireguard_port_min")}
						{field("WireGuard 端口上界", "wireguard_port_max")}
					</div>
					{field(
						"WireGuard 公网端点",
						"wireguard_public_endpoint",
						"选手侧配置里下发的地址；留空表示由服务端推导",
					)}
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<span className="ai-strong">网络健康</span>
					<Divider />
					{health.isLoading ? (
						<LoadingState label="检查中…" />
					) : health.error ? (
						<ErrorState error={health.error} onRetry={() => void health.refetch()} />
					) : health.data ? (
						<div className="ai-col" style={{ gap: 8 }}>
							{Object.entries(health.data).map(([k, v]) => (
								<div key={k} className="ai-row-between">
									<span className="ai-mono">{k}</span>
									{healthTag(typeof v === "string" ? v : v === null ? null : String(v))}
								</div>
							))}
						</div>
					) : (
						<EmptyState title="没有健康数据" />
					)}
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<span className="ai-strong">网段分配</span>
					<Divider />
					{allocations.isLoading ? (
						<LoadingState label="加载分配…" />
					) : allocations.error ? (
						<ErrorState error={allocations.error} onRetry={() => void allocations.refetch()} />
					) : (allocations.data?.length ?? 0) === 0 ? (
						<EmptyState
							title="还没有分配记录"
							description="创建并部署 AWD 赛事后，这里会出现它占用的网段。"
						/>
					) : (
						<div className="ai-scroll">
							<table style={{ width: "100%", borderCollapse: "collapse" }}>
								<thead>
									<tr>
										{["赛事", "类型", "网段", "分配时间", "状态"].map((h) => (
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
									{(allocations.data ?? []).map((a, i) => (
										<tr key={`${a.event_id}-${a.cidr}-${i}`}>
											<td style={{ padding: "8px 12px" }}>
												{a.event_title ?? a.event_id.slice(0, 8)}
											</td>
											<td style={{ padding: "8px 12px" }}>
												<Tag size="small" variant="outlined">
													{a.kind}
												</Tag>
											</td>
											<td style={{ padding: "8px 12px" }} className="ai-mono">
												{a.cidr}
											</td>
											<td style={{ padding: "8px 12px" }} className="ai-muted">
												{formatTime(a.allocated_at)}
											</td>
											<td style={{ padding: "8px 12px" }}>
												{a.active ? (
													<Tag size="small" variant="soft" color="app-green">
														<CheckCircleIcon size={12} /> 使用中
													</Tag>
												) : (
													<Tag size="small" variant="soft" color="brown">
														已释放 {a.released_at ? formatTime(a.released_at) : ""}
													</Tag>
												)}
											</td>
										</tr>
									))}
								</tbody>
							</table>
						</div>
					)}
				</div>
			</Card>
		</>
	);
}
