/**
 * 靶场网络（`/admin/network`，specialized）—— 平台网络设置 / 宿主健康 / 分配容量 / 分配账本。
 *
 * 真实接口（全部只读，除设置外）：
 * - `getPlatformNetwork()`            → `PlatformNetworkSettings`（含服务端计算的容量预览）；
 * - `updatePlatformNetwork(body)`     → 部分更新，返回 `note`（原样展示给管理员）；
 * - `getPlatformNetworkHealth()`      → `PlatformNetworkHealth`（宿主观测状态，只读）；
 * - `getPlatformNetworkAllocations()` → `PlatformNetworkAllocation[]`（分配账本，只读）。
 *
 * `null` / 空值一律显示为空态，不编造默认值；能力（capacity）由后端按**已保存**配置计算，
 * 因此编辑抽屉里不会实时重算容量 —— 保存后刷新即可看到新值。
 */

import { useState, type ReactNode } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Network, Pencil, RefreshCw, ShieldAlert } from "lucide-react";

import { call, callList } from "~/api/call";
import { useAppQueryClient, useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill, type PillTone } from "~/components/app/badges";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { Field, FormFooter, FormGrid, FormSheet } from "~/components/app/form";
import {
	KeyValueList,
	MonoText,
	PageBody,
	PageHeader,
	SectionCard,
	StatCard,
	Toolbar,
} from "~/components/app/page";
import { EmptyBlock, LoadingBlock, QueryState } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import { formatDateTime, formatInt } from "~/lib/format";
import { useDocumentTitle } from "~/lib/hooks";
import type {
	PlatformNetworkAllocation,
	PlatformNetworkHealth,
	PlatformNetworkSettings,
	PlatformNetworkSettingsUpdateResponse,
} from "@floatctf/sdk";

import {
	AWD_MAX_EVENT_PREFIX,
	EMPTY_PLATFORM_NETWORK_FORM,
	type PlatformNetworkErrors,
	type PlatformNetworkForm,
	formFromSettings,
	validatePlatformNetworkForm,
} from "./network-validation";

const ALLOCATION_KIND_LABEL: Record<string, string> = {
	gamebox: "GameBox 网段",
	wireguard: "WireGuard 网段",
};

export function AdminNetworkPage(): ReactNode {
	useDocumentTitle("靶场网络 · FloatCTF");
	const client = useClient();
	const queryClient = useAppQueryClient();

	const settings = useQuery({
		queryKey: qk.awd.platformNetwork(),
		queryFn: () => call<PlatformNetworkSettings>(client.awd.admin.getPlatformNetwork(), "平台网络设置"),
	});

	const health = useQuery({
		queryKey: qk.awd.platformHealth(),
		queryFn: () =>
			call<PlatformNetworkHealth>(client.awd.admin.getPlatformNetworkHealth(), "宿主健康状态"),
	});

	const allocations = useQuery({
		queryKey: qk.awd.platformAllocations(),
		queryFn: () =>
			callList<PlatformNetworkAllocation>(client.awd.admin.getPlatformNetworkAllocations()),
	});

	const [sheetOpen, setSheetOpen] = useState(false);
	const [form, setForm] = useState<PlatformNetworkForm>(EMPTY_PLATFORM_NETWORK_FORM);
	const [errors, setErrors] = useState<PlatformNetworkErrors>({});

	const save = useMutation({
		mutationFn: (body: PlatformNetworkForm) =>
			call<PlatformNetworkSettingsUpdateResponse>(
				client.awd.admin.updatePlatformNetwork({
					gamebox_pool: body.gamebox_pool.trim(),
					gamebox_event_prefix: Number(body.gamebox_event_prefix),
					gamebox_team_prefix: Number(body.gamebox_team_prefix),
					wireguard_pool: body.wireguard_pool.trim(),
					wireguard_event_prefix: Number(body.wireguard_event_prefix),
					wireguard_team_prefix: Number(body.wireguard_team_prefix),
					wireguard_port_min: Number(body.wireguard_port_min),
					wireguard_port_max: Number(body.wireguard_port_max),
					// 空串 = 清空（后端 double_option：显式 null 才清空）。
					wireguard_public_endpoint: body.wireguard_public_endpoint.trim() || null,
				}),
				"保存平台网络设置",
			),
		onSuccess: (data) => {
			// 后端返回的 note 是权威语义说明（现有分配不受影响，仅后续分配生效），原样展示。
			toast.success("平台网络设置已保存", data.note ?? "新的分配将按保存后的配置执行。");
			void queryClient.invalidateQueries({ queryKey: qk.awd.platformNetwork() });
			setSheetOpen(false);
			setErrors({});
		},
		onError: (error) => toast.apiError("保存平台网络设置失败", error),
	});

	function openSettings(): void {
		const data = settings.data;
		if (!data) {
			toast.warning("设置尚未加载完成");
			return;
		}
		setForm(formFromSettings(data));
		setErrors({});
		setSheetOpen(true);
	}

	function submit(): void {
		const nextErrors = validatePlatformNetworkForm(form);
		setErrors(nextErrors);
		if (Object.keys(nextErrors).length > 0) {
			toast.warning("请先修正表单中的错误");
			return;
		}
		save.mutate(form);
	}

	function refreshAll(): void {
		void settings.refetch();
		void health.refetch();
		void allocations.refetch();
	}

	const refreshing = settings.isFetching || health.isFetching || allocations.isFetching;

	return (
		<PageBody>
			<PageHeader
				title="靶场网络"
				description="平台级 AWD 网络地址池、宿主网络能力观测与分配账本。地址池变更只影响后续分配。"
				actions={
					<Toolbar>
						<Button variant="outline" onClick={refreshAll} disabled={refreshing}>
							<RefreshCw className={refreshing ? "animate-spin" : undefined} />
							刷新
						</Button>
						<Button onClick={openSettings} disabled={!settings.data}>
							<Pencil /> 编辑设置
						</Button>
					</Toolbar>
				}
			/>

			<SectionCard
				title="平台地址池设置"
				description="GameBox 与 WireGuard 各自独立的地址池与子网长度；容量由后端按已保存配置计算。"
			>
				<QueryState
					query={settings}
					skeleton={<LoadingBlock label="加载网络设置…" />}
					errorTitle="加载平台网络设置失败"
				>
					{(data) => (
						<div className="space-y-5">
							<KeyValueList
								columns={3}
								items={[
									{
										key: "GameBox 地址池",
										value: <MonoText className="text-sm">{data.gamebox_pool}</MonoText>,
										hint: `赛事子网 /${data.gamebox_event_prefix} · 队伍子网 /${data.gamebox_team_prefix}`,
									},
									{
										key: "WireGuard 地址池",
										value: <MonoText className="text-sm">{data.wireguard_pool}</MonoText>,
										hint: `赛事子网 /${data.wireguard_event_prefix} · 队伍子网 /${data.wireguard_team_prefix}`,
									},
									{
										key: "WireGuard 端口范围",
										value: (
											<MonoText className="text-sm">
												{data.wireguard_port_min}–{data.wireguard_port_max}
											</MonoText>
										),
									},
									{
										key: "公共接入端点",
										value: data.wireguard_public_endpoint ? (
											<MonoText className="text-sm">{data.wireguard_public_endpoint}</MonoText>
										) : (
											<span className="text-xs text-muted-foreground">未配置</span>
										),
										hint: "下发队伍配置时使用的对外地址；留空表示由客户端自行决定。",
									},
									{
										key: "最近更新",
										value: <MonoText className="text-sm">{formatDateTime(data.updated_at, { seconds: true })}</MonoText>,
									},
								]}
							/>

							<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
								<StatCard
									label="GameBox 赛事容量"
									value={formatInt(data.gamebox_event_capacity)}
									hint="按地址池长度可容纳的赛事数"
									icon={<Network className="size-5" />}
								/>
								<StatCard
									label="每赛事队伍容量"
									value={formatInt(data.gamebox_team_capacity_per_event)}
									hint="GameBox 网段内的队伍数上限"
								/>
								<StatCard
									label="每队靶机数"
									value={formatInt(data.gamebox_hosts_per_team)}
									hint="每支队伍可挂载的 GameBox 数量"
								/>
								<StatCard
									label="WireGuard 赛事容量"
									value={formatInt(data.wireguard_event_capacity)}
									hint="按隧道地址池计算的赛事数"
								/>
								<StatCard
									label="每赛事队伍容量"
									value={formatInt(data.wireguard_team_capacity_per_event)}
									hint="WireGuard 网段内的队伍数上限"
								/>
								<StatCard
									label="端口容量"
									value={formatInt(data.wireguard_port_capacity)}
									hint="当前端口范围内可分配的中继端口数"
								/>
							</div>
						</div>
					)}
				</QueryState>
			</SectionCard>

			<SectionCard
				title="宿主网络能力"
				description="只读观测：宿主 nftables / WireGuard / Docker 防火墙后端等实际情况。"
				actions={
					<TonePill
						tone={
							health.data === undefined
								? "muted"
								: health.data.capability_supported
									? "success"
									: "danger"
						}
					>
						{health.data === undefined
							? "未探测"
							: health.data.capability_supported
								? "宿主能力满足"
								: "宿主能力不满足"}
					</TonePill>
				}
			>
				<QueryState
					query={health}
					skeleton={<LoadingBlock label="探测宿主网络能力…" />}
					errorTitle="加载宿主健康状态失败"
				>
					{(data) => (
						<div className="space-y-4">
							<div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
								{buildHealthEntries(data).map((entry) => (
									<div key={entry.keyName} className="space-y-1.5 rounded-lg border p-3">
										<div className="flex items-center justify-between gap-2">
											<span className="text-sm font-medium">{entry.label}</span>
											<TonePill tone={entry.tone}>{entry.status}</TonePill>
										</div>
										<MonoText className="text-muted-foreground">{entry.keyName}</MonoText>
										{entry.raw && entry.raw !== entry.status ? (
											<MonoText className="text-muted-foreground">{entry.raw}</MonoText>
										) : null}
										<p className="text-xs text-muted-foreground">{entry.hint}</p>
									</div>
								))}
							</div>
							{data.notes.length > 0 ? (
								<div className="space-y-1 rounded-lg border border-[var(--warning)]/40 bg-[var(--warning)]/10 p-3">
									<p className="flex items-center gap-1.5 text-sm font-medium">
										<ShieldAlert className="size-4" /> 后端观测备注
									</p>
									<ul className="list-disc space-y-0.5 pl-5 text-xs text-muted-foreground">
										{data.notes.map((note) => (
											<li key={note}>{note}</li>
										))}
									</ul>
								</div>
							) : null}
						</div>
					)}
				</QueryState>
			</SectionCard>

			<SectionCard
				title="地址分配账本"
				description="平台的网段分配记录（接口一次性返回全部，因此本表不分页、不提供搜索）。"
				contentClassName="p-0"
				actions={
					allocations.data ? (
						<span className="tnum text-xs text-muted-foreground">
							共 {allocations.data.items.length} 条 ·{" "}
							{allocations.data.items.filter((item) => item.active).length} 条使用中
						</span>
					) : null
				}
			>
				<QueryState
					query={allocations}
					skeleton={<LoadingBlock label="加载分配账本…" />}
					errorTitle="加载分配账本失败"
					isEmpty={(result) => result.items.length === 0}
					empty={
						<EmptyBlock
							title="还没有地址分配记录"
							description="为赛事分配 AWD 网络后，这里会出现对应的网段记录。"
						/>
					}
				>
					{(result) => (
						<DataTable<PlatformNetworkAllocation>
							data={result.items}
							getRowId={(row) => `${row.event_id}-${row.kind}-${row.cidr}`}
							columns={allocationColumns}
							mobileCard={(row) => (
								<div className="space-y-1">
									<div className="flex items-center justify-between gap-2">
										<span className="text-sm font-medium">{row.event_title ?? row.event_id}</span>
										<TonePill tone={row.active ? "success" : "muted"}>
											{row.active ? "使用中" : "已释放"}
										</TonePill>
									</div>
									<MonoText>
										{ALLOCATION_KIND_LABEL[row.kind] ?? row.kind} · {row.cidr}
									</MonoText>
									<p className="tnum text-xs text-muted-foreground">
										分配 {formatDateTime(row.allocated_at)}
										{row.released_at ? ` · 释放 ${formatDateTime(row.released_at)}` : ""}
									</p>
								</div>
							)}
							empty={<span>暂无分配记录</span>}
						/>
					)}
				</QueryState>
			</SectionCard>

			<FormSheet
				open={sheetOpen}
				onOpenChange={(open) => {
					if (!open) setSheetOpen(false);
				}}
				title="编辑平台网络设置"
				description="保存后仅影响**后续**的赛事网络分配；已分配的赛事保持原网段，不会被打断。"
				width="xl"
				footer={
					<FormFooter
						onCancel={() => setSheetOpen(false)}
						submitLabel="保存设置"
						isPending={save.isPending}
						formId="admin-network-form"
						hint={`赛事网段长度不得超过 /${AWD_MAX_EVENT_PREFIX}（AWD 预检要求）`}
					/>
				}
			>
				<form
					id="admin-network-form"
					className="space-y-4"
					onSubmit={(event) => {
						event.preventDefault();
						submit();
					}}
				>
					<FormGrid columns={2}>
						<Field
							label="GameBox 地址池"
							htmlFor="network-gamebox-pool"
							required
							error={errors.gamebox_pool}
							hint="CIDR；必须是网络地址（主机位为 0）。"
						>
							<Input
								id="network-gamebox-pool"
								className="font-mono text-xs"
								value={form.gamebox_pool}
								onChange={(event) => setForm({ ...form, gamebox_pool: event.target.value })}
							/>
						</Field>
						<Field
							label="WireGuard 地址池"
							htmlFor="network-wireguard-pool"
							required
							error={errors.wireguard_pool}
							hint="与 GameBox 地址池不得重叠。"
						>
							<Input
								id="network-wireguard-pool"
								className="font-mono text-xs"
								value={form.wireguard_pool}
								onChange={(event) => setForm({ ...form, wireguard_pool: event.target.value })}
							/>
						</Field>
						<Field
							label="GameBox 赛事子网长度"
							htmlFor="network-gamebox-event-prefix"
							required
							error={errors.gamebox_event_prefix}
							hint={`不小于地址池长度，且不得超过 ${AWD_MAX_EVENT_PREFIX}。`}
						>
							<Input
								id="network-gamebox-event-prefix"
								inputMode="numeric"
								value={form.gamebox_event_prefix}
								onChange={(event) => setForm({ ...form, gamebox_event_prefix: event.target.value })}
							/>
						</Field>
						<Field
							label="GameBox 队伍子网长度"
							htmlFor="network-gamebox-team-prefix"
							required
							error={errors.gamebox_team_prefix}
							hint="不得小于赛事子网长度。"
						>
							<Input
								id="network-gamebox-team-prefix"
								inputMode="numeric"
								value={form.gamebox_team_prefix}
								onChange={(event) => setForm({ ...form, gamebox_team_prefix: event.target.value })}
							/>
						</Field>
						<Field
							label="WireGuard 赛事子网长度"
							htmlFor="network-wireguard-event-prefix"
							required
							error={errors.wireguard_event_prefix}
							hint="不小于地址池长度。"
						>
							<Input
								id="network-wireguard-event-prefix"
								inputMode="numeric"
								value={form.wireguard_event_prefix}
								onChange={(event) => setForm({ ...form, wireguard_event_prefix: event.target.value })}
							/>
						</Field>
						<Field
							label="WireGuard 队伍子网长度"
							htmlFor="network-wireguard-team-prefix"
							required
							error={errors.wireguard_team_prefix}
							hint="不得小于赛事子网长度。"
						>
							<Input
								id="network-wireguard-team-prefix"
								inputMode="numeric"
								value={form.wireguard_team_prefix}
								onChange={(event) => setForm({ ...form, wireguard_team_prefix: event.target.value })}
							/>
						</Field>
						<Field
							label="WireGuard 起始端口"
							htmlFor="network-port-min"
							required
							error={errors.wireguard_port_min}
							hint="1–65535。"
						>
							<Input
								id="network-port-min"
								inputMode="numeric"
								value={form.wireguard_port_min}
								onChange={(event) => setForm({ ...form, wireguard_port_min: event.target.value })}
							/>
						</Field>
						<Field
							label="WireGuard 结束端口"
							htmlFor="network-port-max"
							required
							error={errors.wireguard_port_max}
							hint="不得小于起始端口。"
						>
							<Input
								id="network-port-max"
								inputMode="numeric"
								value={form.wireguard_port_max}
								onChange={(event) => setForm({ ...form, wireguard_port_max: event.target.value })}
							/>
						</Field>
					</FormGrid>
					<Field
						label="公共接入端点"
						htmlFor="network-endpoint"
						error={errors.wireguard_public_endpoint}
						hint="形式为「IP 或域名[:端口]」，端口可省略；留空表示清空。"
					>
						<Input
							id="network-endpoint"
							className="font-mono text-xs"
							placeholder="vpn.example.com:51820"
							value={form.wireguard_public_endpoint}
							onChange={(event) =>
								setForm({ ...form, wireguard_public_endpoint: event.target.value })
							}
						/>
					</Field>
				</form>
			</FormSheet>
		</PageBody>
	);
}

// ── 宿主健康条目（把后端原始观测值翻译成可读状态；原始值同时展示）──────────

interface HealthEntry {
	label: string;
	keyName: string;
	tone: PillTone;
	status: string;
	raw?: string | null;
	hint: string;
}

function buildHealthEntries(health: PlatformNetworkHealth): HealthEntry[] {
	const isHealthy = (raw: string): boolean => raw.toLowerCase().startsWith("healthy");
	return [
		{
			label: "防火墙工具",
			keyName: "nftables",
			tone: isHealthy(health.nftables) ? "success" : "danger",
			status: isHealthy(health.nftables) ? "正常" : "缺失",
			raw: health.nftables,
			hint: "AWD 网络规则由宿主 nftables 承载。",
		},
		{
			label: "WireGuard 内核支持",
			keyName: "wireguard",
			tone: isHealthy(health.wireguard) ? "success" : "danger",
			status: isHealthy(health.wireguard) ? "正常" : "缺失",
			raw: health.wireguard,
			hint: "缺失时无法建立队伍隧道，需要在宿主机加载 wireguard 模块。",
		},
		{
			label: "容器网络能力",
			keyName: "docker",
			tone:
				health.docker.toLowerCase() === "available"
					? "success"
					: health.docker.toLowerCase() === "unknown"
						? "warning"
						: "danger",
			status:
				health.docker.toLowerCase() === "available"
					? "可用"
					: health.docker.toLowerCase() === "unknown"
						? "未知"
						: "不可用",
			raw: health.docker,
			hint: "依据宿主 IPv4 转发状态判断容器网络是否可用。",
		},
		{
			label: "防火墙运行时",
			keyName: "firewall_runtime",
			tone: health.firewall_runtime.includes("nftables") ? "success" : "warning",
			status: health.firewall_runtime,
			hint: "平台使用原生 nftables 规则集，不依赖其它防火墙管理工具。",
		},
		{
			label: "平台防火墙表",
			keyName: "floatctf_table",
			tone: "neutral",
			status: health.floatctf_table,
			hint: "平台规则所在的 nftables 表名；清空该表会中断全部 AWD 网络。",
		},
		{
			label: "Docker 防火墙后端",
			keyName: "docker_firewall_backend",
			tone: health.docker_firewall_backend ? "success" : "muted",
			status: health.docker_firewall_backend ?? "未探测到",
			raw: health.docker_firewall_backend,
			hint: "宿主 Docker 使用的防火墙后端，需要与平台 nftables 规则共存。",
		},
		{
			label: "firewalld 服务",
			keyName: "firewalld",
			tone: health.firewalld.toLowerCase() === "active" ? "warning" : "success",
			status: health.firewalld.toLowerCase() === "active" ? "运行中" : "未运行",
			raw: health.firewalld,
			hint: "firewalld 与平台 nftables 规则可能互相覆盖，建议保持未运行。",
		},
		{
			label: "IPv4 转发",
			keyName: "ipv4_forwarding",
			tone:
				health.ipv4_forwarding === "enabled"
					? "success"
					: health.ipv4_forwarding === "disabled"
						? "danger"
						: "warning",
			status:
				health.ipv4_forwarding === "enabled"
					? "已启用"
					: health.ipv4_forwarding === "disabled"
						? "未启用"
						: "未知",
			raw: health.ipv4_forwarding,
			hint: "队伍网段与容器互通依赖该内核参数。",
		},
		{
			label: "IPv6 策略",
			keyName: "ipv6_policy",
			tone: health.ipv6_policy === "blocked" ? "success" : "warning",
			status: health.ipv6_policy === "blocked" ? "已阻断" : health.ipv6_policy,
			raw: health.ipv6_policy,
			hint: "AWD 不提供 IPv6 路由，v6 流量默认丢弃。",
		},
		{
			label: "宿主能力检测",
			keyName: "capability_supported",
			tone: health.capability_supported ? "success" : "danger",
			status: health.capability_supported ? "满足" : "不满足（网络分配可能失败）",
			hint: "宿主机是否具备 AWD 网络所需的全部能力。",
		},
	];
}

// ── 分配账本列 ──────────────────────────────────────────────────────────────

const allocationColumns: DataTableColumn<PlatformNetworkAllocation>[] = [
	{
		id: "event",
		header: "赛事",
		cell: (row) => (
			<div className="min-w-0">
				<p className="truncate text-sm">{row.event_title ?? "（赛事已删除或标题缺失）"}</p>
				<MonoText className="text-muted-foreground">{row.event_id}</MonoText>
			</div>
		),
		sortValue: (row) => row.event_title ?? row.event_id,
	},
	{
		id: "kind",
		header: "类型",
		cell: (row) => (
			<span className="text-sm">{ALLOCATION_KIND_LABEL[row.kind] ?? row.kind}</span>
		),
		sortValue: (row) => row.kind,
	},
	{
		id: "cidr",
		header: "网段",
		cell: (row) => <MonoText className="text-sm">{row.cidr}</MonoText>,
		sortValue: (row) => row.cidr,
	},
	{
		id: "active",
		header: "状态",
		cell: (row) => (
			<TonePill tone={row.active ? "success" : "muted"}>{row.active ? "使用中" : "已释放"}</TonePill>
		),
		sortValue: (row) => (row.active ? 1 : 0),
	},
	{
		id: "allocated_at",
		header: "分配时间",
		cell: (row) => (
			<span className="tnum text-xs text-muted-foreground">{formatDateTime(row.allocated_at)}</span>
		),
		sortValue: (row) => Date.parse(row.allocated_at),
		hideBelow: "md",
	},
	{
		id: "released_at",
		header: "释放时间",
		cell: (row) =>
			row.released_at ? (
				<span className="tnum text-xs text-muted-foreground">{formatDateTime(row.released_at)}</span>
			) : (
				<span className="text-xs text-muted-foreground">—</span>
			),
		sortValue: (row) => (row.released_at ? Date.parse(row.released_at) : 0),
		hideBelow: "lg",
	},
];
