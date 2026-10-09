import {
	Box,
	Button,
	FormControl,
	Label,
	Spinner,
	TextInput,
} from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import {
	type ChangeEvent,
	type ReactNode,
	useEffect,
	useMemo,
	useRef,
	useState,
} from "react";

import { adminApi } from "@/api";
import type {
	PlatformNetworkAllocation,
	PlatformNetworkHealth,
	PlatformNetworkSettings,
} from "@floatctf/sdk";
import type { QueryParams, UniResponse } from "@floatctf/sdk";
import { GenericTable, useMsgBanner } from "@/components";
import {
	EMPTY_PLATFORM_NETWORK_FORM,
	type PlatformNetworkErrors,
	type PlatformNetworkForm,
	hasErrors,
	isSamePlatformForm,
	platformFormFromSettings,
	validatePlatformNetworkForm,
} from "@/components/awd/networkForm";
import { DatetimeToShow } from "@/util";
import { AdminRouteGuard } from "../route";

export const Route = createFileRoute("/admin/awd/network")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

const SETTINGS_KEY = "AWDPlatformNetworkSettings";
const HEALTH_KEY = "AWDPlatformNetworkHealth";
const ALLOCATIONS_KEY = "AWDPlatformNetworkAllocations";

const FIELD_GRID: React.CSSProperties = {
	display: "grid",
	gridTemplateColumns: "repeat(auto-fit, minmax(260px, 1fr))",
	gap: 16,
};

/** 表单分区：标题 + 说明 + 内容。 */
function Section({
	title,
	description,
	children,
}: {
	title: string;
	description?: string;
	children: ReactNode;
}) {
	return (
		<section className="mt-4">
			<h4 className="mb-1">{title}</h4>
			{description && (
				<p className="color-fg-muted mb-2 text-sm">{description}</p>
			)}
			{children}
		</section>
	);
}

/** 配置项：中文名称 + 原始配置键 + 说明 + 校验提示。 */
function Field({
	label,
	keyName,
	caption,
	value,
	onChange,
	placeholder,
	monospace,
	type = "text",
	min,
	max,
	error,
}: {
	label: string;
	keyName: string;
	caption: string;
	value: string;
	onChange: (event: ChangeEvent<HTMLInputElement>) => void;
	placeholder?: string;
	monospace?: boolean;
	type?: "text" | "number";
	min?: number;
	max?: number;
	error?: string;
}) {
	return (
		<FormControl>
			<FormControl.Label>{label}</FormControl.Label>
			<FormControl.Caption>
				<span className="block font-mono text-xs">{keyName}</span>
				<span className="block">{caption}</span>
			</FormControl.Caption>
			<TextInput
				type={type}
				value={value}
				onChange={onChange}
				placeholder={placeholder}
				monospace={monospace}
				min={min}
				max={max}
				step={type === "number" ? 1 : undefined}
				block
				validationStatus={error ? "error" : undefined}
			/>
			{error && (
				<FormControl.Validation variant="error">{error}</FormControl.Validation>
			)}
		</FormControl>
	);
}

/** 容量指标：数值由后端按已保存配置计算。 */
function CapacityItem({
	label,
	value,
	hint,
}: {
	label: string;
	value: number;
	hint?: string;
}) {
	return (
		<div>
			<div className="text-sm color-fg-muted">{label}</div>
			<div className="font-mono text-lg font-semibold">{value}</div>
			{hint && <div className="text-xs color-fg-muted">{hint}</div>}
		</div>
	);
}

type HealthTone = "success" | "attention" | "danger" | "default";

type HealthEntry = {
	label: string;
	keyName: string;
	tone: HealthTone;
	status: string;
	raw?: string | null;
	hint: string;
};

function healthState(
	ok: boolean,
	okText: string,
	failText: string,
): { tone: HealthTone; status: string } {
	return ok
		? { tone: "success", status: okText }
		: { tone: "danger", status: failText };
}

/** 把后端返回的宿主检测原始值翻译为可读状态。 */
function buildHealthEntries(health: PlatformNetworkHealth): HealthEntry[] {
	const isHealthy = (raw: string | null | undefined) =>
		Boolean(raw?.toLowerCase().startsWith("healthy"));
	const entries: HealthEntry[] = [
		{
			label: "防火墙工具",
			keyName: "nftables",
			...healthState(isHealthy(health.nftables), "正常", "缺失"),
			raw: health.nftables,
			hint: "AWD 的网络规则由宿主机 nftables 承载。",
		},
		{
			label: "WireGuard 内核支持",
			keyName: "wireguard",
			...healthState(isHealthy(health.wireguard), "正常", "缺失"),
			raw: health.wireguard,
			hint: "缺失时无法建立队伍隧道，需在宿主机加载 wireguard 内核模块。",
		},
		{
			label: "容器网络能力",
			keyName: "docker",
			...healthState(
				health.docker.toLowerCase() === "available",
				"可用",
				health.docker.toLowerCase() === "unknown" ? "未知" : "不可用",
			),
			raw: health.docker,
			hint: "依据宿主机 IPv4 转发状态判断容器网络是否可用。",
		},
		{
			label: "防火墙运行时",
			keyName: "firewall_runtime",
			tone: health.firewall_runtime.includes("nftables")
				? "success"
				: "attention",
			status: health.firewall_runtime,
			hint: "平台使用原生 nftables 规则集，不依赖其他防火墙管理工具。",
		},
		{
			label: "平台防火墙表",
			keyName: "floatctf_table",
			tone: "success",
			status: health.floatctf_table,
			hint: "平台规则所在的 nftables 表名；清空该表会中断全部 AWD 网络。",
		},
		{
			label: "Docker 防火墙后端",
			keyName: "docker_firewall_backend",
			...healthState(
				Boolean(health.docker_firewall_backend),
				health.docker_firewall_backend ?? "未知",
				"未探测到",
			),
			hint: "宿主机 Docker 使用的防火墙后端，需与平台 nftables 规则共存。",
		},
		{
			label: "firewalld 服务",
			keyName: "firewalld",
			tone:
				health.firewalld.toLowerCase() === "active" ? "attention" : "success",
			status: health.firewalld.toLowerCase() === "active" ? "运行中" : "未运行",
			raw: health.firewalld,
			hint: "firewalld 与平台 nftables 规则可能相互覆盖，建议保持未运行。",
		},
		{
			label: "IPv4 转发",
			keyName: "ipv4_forwarding",
			tone:
				health.ipv4_forwarding === "enabled"
					? "success"
					: health.ipv4_forwarding === "disabled"
						? "danger"
						: "attention",
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
			tone: health.ipv6_policy === "blocked" ? "success" : "attention",
			status: health.ipv6_policy === "blocked" ? "已阻断" : health.ipv6_policy,
			raw: health.ipv6_policy,
			hint: "AWD 不提供 IPv6 路由，v6 流量默认丢弃。",
		},
		{
			label: "宿主能力检测",
			keyName: "capability_supported",
			...healthState(
				health.capability_supported,
				"满足",
				"不满足（网络分配可能失败）",
			),
			hint: "宿主机是否具备 AWD 网络所需的全部能力。",
		},
	];
	return entries;
}

/** 宿主检测卡片（只读）。 */
function HealthCard({ entry }: { entry: HealthEntry }) {
	return (
		<div
			style={{
				border: "1px solid",
				borderColor: "var(--borderColor-default)",
				borderRadius: 6,
				padding: 12,
			}}
		>
			<div className="text-sm font-semibold">{entry.label}</div>
			<div className="font-mono text-xs color-fg-muted">{entry.keyName}</div>
			<div className="mt-1">
				<Label variant={entry.tone}>{entry.status}</Label>
			</div>
			{entry.raw && entry.raw !== entry.status && (
				<div className="mt-1 font-mono text-xs color-fg-muted break-all">
					{entry.raw}
				</div>
			)}
			<div className="mt-1 text-xs color-fg-muted">{entry.hint}</div>
		</div>
	);
}

function RouteComponent() {
	const qc = useQueryClient();
	const banner = useMsgBanner({});

	const settings = useQuery({
		queryKey: [SETTINGS_KEY],
		queryFn: () => adminApi.awd.getPlatformNetwork(),
		staleTime: 30_000,
	});
	const health = useQuery({
		queryKey: [HEALTH_KEY],
		queryFn: () => adminApi.awd.getPlatformNetworkHealth(),
		staleTime: 30_000,
	});

	const savedSettings: PlatformNetworkSettings | undefined =
		settings.data?.data;
	const savedForm = useMemo(
		() => (savedSettings ? platformFormFromSettings(savedSettings) : undefined),
		[savedSettings],
	);

	const [form, setForm] = useState<PlatformNetworkForm>(
		EMPTY_PLATFORM_NETWORK_FORM,
	);
	// 只在首次拿到配置时回填，避免后台刷新覆盖未保存的编辑
	const initialized = useRef(false);
	useEffect(() => {
		if (savedSettings && !initialized.current) {
			initialized.current = true;
			setForm(platformFormFromSettings(savedSettings));
		}
	}, [savedSettings]);

	const errors: PlatformNetworkErrors = useMemo(
		() => validatePlatformNetworkForm(form),
		[form],
	);
	const dirty = savedForm ? !isSamePlatformForm(form, savedForm) : false;

	const setField =
		(key: keyof PlatformNetworkForm) =>
		(event: ChangeEvent<HTMLInputElement>) => {
			setForm((prev) => ({ ...prev, [key]: event.target.value }));
		};

	const save = useMutation({
		mutationFn: () =>
			adminApi.awd.updatePlatformNetwork({
				gamebox_pool: form.gamebox_pool.trim(),
				gamebox_event_prefix: Number(form.gamebox_event_prefix),
				gamebox_team_prefix: Number(form.gamebox_team_prefix),
				wireguard_pool: form.wireguard_pool.trim(),
				wireguard_event_prefix: Number(form.wireguard_event_prefix),
				wireguard_team_prefix: Number(form.wireguard_team_prefix),
				wireguard_public_endpoint:
					form.wireguard_public_endpoint.trim() || null,
				wireguard_port_min: Number(form.wireguard_port_min),
				wireguard_port_max: Number(form.wireguard_port_max),
			}),
		onSuccess: () => {
			banner.showBanner(
				"success",
				"网络配置已保存。新的分配按保存后的配置执行，已分配的赛事不受影响。",
			);
			initialized.current = false;
			qc.invalidateQueries({ queryKey: [SETTINGS_KEY] });
		},
		onError: (e) => banner.showErrorBanner(e),
	});

	const allocationQueryFn = async (
		params?: QueryParams,
	): Promise<UniResponse<PlatformNetworkAllocation[]>> => {
		const res = await adminApi.awd.getPlatformNetworkAllocations();
		const data = res.data ?? [];
		return { ...res, data, meta: { ...params, total: data.length } };
	};
	const allocationColumns = [
		{
			accessorKey: "event_id",
			header: "赛事",
			field: "event_id",
			renderCell: (row: PlatformNetworkAllocation) => (
				<span>
					{row.event_title ?? row.event_id}
					{row.event_title && (
						<span className="color-fg-muted font-mono text-xs">
							{" "}
							{row.event_id}
						</span>
					)}
				</span>
			),
		},
		{
			accessorKey: "kind",
			header: "类型",
			field: "kind",
			renderCell: (row: PlatformNetworkAllocation) => (
				<span>
					{row.kind === "gamebox"
						? "GameBox 网段"
						: row.kind === "wireguard"
							? "WireGuard 网段"
							: row.kind}
				</span>
			),
		},
		{
			accessorKey: "cidr",
			header: "网段",
			field: "cidr",
			renderCell: (row: PlatformNetworkAllocation) => (
				<span className="font-mono">{row.cidr}</span>
			),
		},
		{
			accessorKey: "active",
			header: "状态",
			field: "active",
			renderCell: (row: PlatformNetworkAllocation) => (
				<Label variant={row.active ? "success" : "default"}>
					{row.active ? "使用中" : "已释放"}
				</Label>
			),
		},
		{
			accessorKey: "allocated_at",
			header: "分配时间",
			field: "allocated_at",
			renderCell: (row: PlatformNetworkAllocation) => (
				<span>{DatetimeToShow(row.allocated_at)}</span>
			),
		},
		{
			accessorKey: "released_at",
			header: "释放时间",
			field: "released_at",
			renderCell: (row: PlatformNetworkAllocation) => (
				<span>{DatetimeToShow(row.released_at)}</span>
			),
		},
	];

	const healthData: PlatformNetworkHealth | undefined = health.data?.data;
	const healthEntries = healthData ? buildHealthEntries(healthData) : [];

	return (
		<div className="mt-3" style={{ maxWidth: 1100 }}>
			<banner.BannerComponent className="mb-3" />
			<Box
				sx={{
					p: 4,
					border: "1px solid",
					borderColor: "border.default",
					borderRadius: 2,
				}}
			>
				<div className="d-flex flex-items-center flex-justify-between">
					<div>
						<h3 className="m-0">平台网络配置</h3>
						<p className="color-fg-muted mb-0 mt-1">
							{
								"AWD 网络在平台层面的默认配置：GameBox 与 WireGuard 的地址池划分方式，以及玩家接入 WireGuard 所需的公开参数。"
							}
						</p>
					</div>
					<Label variant="accent">平台级设置</Label>
				</div>

				{settings.isError ? (
					<p className="color-fg-danger mt-4">
						配置加载失败，请刷新页面后重试。
					</p>
				) : settings.isLoading ? (
					<Spinner size="large" />
				) : (
					<>
						<Section
							title="地址池"
							description="GameBox 与 WireGuard 各使用一个独立地址池。系统按两个层级划分网段：每个赛事划分一个赛事子网，赛事内每支队伍划分一个队伍子网。子网长度填写掩码位数（例如 24 表示 /24）。"
						>
							<h5 className="mb-2 mt-3">GameBox 网段</h5>
							<div style={FIELD_GRID}>
								<Field
									label="地址池"
									keyName="gamebox_pool"
									caption="赛事容器网段的来源地址池，例如 10.10.0.0/16。"
									value={form.gamebox_pool}
									onChange={setField("gamebox_pool")}
									placeholder="10.10.0.0/16"
									monospace
									error={errors.gamebox_pool}
								/>
								<Field
									label="赛事子网长度"
									keyName="gamebox_event_prefix"
									caption="每个赛事从地址池中划分的子网长度，不得小于地址池长度，且不得超过 16（AWD 运行时要求赛事网段为 /16 或更大）。"
									value={form.gamebox_event_prefix}
									onChange={setField("gamebox_event_prefix")}
									placeholder="16"
									type="number"
									min={0}
									max={32}
									error={errors.gamebox_event_prefix}
								/>
								<Field
									label="队伍子网长度"
									keyName="gamebox_team_prefix"
									caption="赛事内每支队伍的子网长度，不得小于赛事子网长度；每个赛事的首个子网保留给平台基础设施。"
									value={form.gamebox_team_prefix}
									onChange={setField("gamebox_team_prefix")}
									placeholder="24"
									type="number"
									min={0}
									max={32}
									error={errors.gamebox_team_prefix}
								/>
							</div>

							<h5 className="mb-2 mt-4">WireGuard 网段</h5>
							<div style={FIELD_GRID}>
								<Field
									label="地址池"
									keyName="wireguard_pool"
									caption="队伍 WireGuard 隧道网段的来源地址池，例如 10.20.0.0/16。"
									value={form.wireguard_pool}
									onChange={setField("wireguard_pool")}
									placeholder="10.20.0.0/16"
									monospace
									error={errors.wireguard_pool}
								/>
								<Field
									label="赛事子网长度"
									keyName="wireguard_event_prefix"
									caption="每个赛事从地址池中划分的子网长度，不得小于地址池长度。"
									value={form.wireguard_event_prefix}
									onChange={setField("wireguard_event_prefix")}
									placeholder="20"
									type="number"
									min={0}
									max={32}
									error={errors.wireguard_event_prefix}
								/>
								<Field
									label="队伍子网长度"
									keyName="wireguard_team_prefix"
									caption="赛事内每支队伍的隧道子网长度，不得小于赛事子网长度。"
									value={form.wireguard_team_prefix}
									onChange={setField("wireguard_team_prefix")}
									placeholder="24"
									type="number"
									min={0}
									max={32}
									error={errors.wireguard_team_prefix}
								/>
							</div>
						</Section>

						<Section
							title="容量概览"
							description="以下数值由已保存的配置计算，用于判断地址池与子网长度是否满足赛事规模。"
						>
							{savedSettings ? (
								<>
									<div className="d-flex flex-items-center gap-2 mb-2">
										<Label variant="default">依据已保存配置</Label>
										{dirty && (
											<Label variant="attention">表单已修改，尚未保存</Label>
										)}
									</div>
									<div style={FIELD_GRID}>
										<CapacityItem
											label="GameBox 可容纳赛事数"
											value={savedSettings.gamebox_event_capacity}
										/>
										<CapacityItem
											label="GameBox 每赛事队伍数"
											value={savedSettings.gamebox_team_capacity_per_event}
											hint="每个赛事的首个子网保留给平台基础设施。"
										/>
										<CapacityItem
											label="GameBox 每队地址总数"
											value={savedSettings.gamebox_hosts_per_team}
											hint="含网络地址、网关与广播地址，均不分配给容器。"
										/>
										<CapacityItem
											label="WireGuard 可容纳赛事数"
											value={savedSettings.wireguard_event_capacity}
										/>
										<CapacityItem
											label="WireGuard 每赛事队伍数"
											value={savedSettings.wireguard_team_capacity_per_event}
										/>
										<CapacityItem
											label="WireGuard 可用端口数"
											value={savedSettings.wireguard_port_capacity}
											hint="每个赛事占用一个监听端口。"
										/>
									</div>
									<p className="color-fg-muted mt-2 mb-0 text-xs">
										最后保存时间：
										{DatetimeToShow(savedSettings.updated_at)}
									</p>
								</>
							) : (
								<p className="color-fg-muted mb-0 text-sm">
									暂无已保存的配置。
								</p>
							)}
						</Section>

						<Section
							title="WireGuard 接入"
							description="玩家使用 WireGuard 客户端接入赛事网络时使用的公开参数。"
						>
							<div style={FIELD_GRID}>
								<Field
									label="公开接入地址"
									keyName="wireguard_public_endpoint"
									caption="格式 host:port，用于记录对玩家公开的接入地址。玩家 WireGuard 配置中的接入地址取自系统设置 NODE_IP，端口取自各赛事分配到的监听端口，本项不参与配置下发。"
									value={form.wireguard_public_endpoint}
									onChange={setField("wireguard_public_endpoint")}
									placeholder="vpn.example.com:51820"
									monospace
									error={errors.wireguard_public_endpoint}
								/>
								<Field
									label="起始端口"
									keyName="wireguard_port_min"
									caption="为赛事分配的 WireGuard 监听端口范围下限，例如 51820。"
									value={form.wireguard_port_min}
									onChange={setField("wireguard_port_min")}
									placeholder="51820"
									type="number"
									min={1}
									max={65535}
									error={errors.wireguard_port_min}
								/>
								<Field
									label="结束端口"
									keyName="wireguard_port_max"
									caption="监听端口范围上限（含该端口）；范围越宽，可同时运行的赛事越多。"
									value={form.wireguard_port_max}
									onChange={setField("wireguard_port_max")}
									placeholder="51830"
									type="number"
									min={1}
									max={65535}
									error={errors.wireguard_port_max}
								/>
							</div>
						</Section>

						<Box
							sx={{ mt: 4 }}
							className="d-flex flex-items-center gap-3 flex-wrap"
						>
							<Button
								variant="primary"
								disabled={!dirty || hasErrors(errors) || save.isPending}
								onClick={() => save.mutate()}
							>
								{save.isPending ? "保存中…" : "保存配置"}
							</Button>
							{dirty && (
								<Button
									disabled={save.isPending}
									onClick={() => savedForm && setForm(savedForm)}
								>
									放弃修改
								</Button>
							)}
							<span className="color-fg-muted text-sm">
								{dirty
									? "存在未保存的修改；保存后仅对新建的网络分配生效，已分配的赛事不受影响。"
									: "当前配置已保存；修改仅对新建的网络分配生效。"}
							</span>
						</Box>

						<Section
							title="当前网络分配"
							description="平台已分配给各赛事的独占网段"
						>
							<GenericTable
								subject={ALLOCATIONS_KEY}
								// 区块标题已是「当前网络分配」，隐藏 GenericTable 的内部键名标题。
								hideTitle
								columns={allocationColumns}
								queryFn={allocationQueryFn}
								getRowId={(row) => `${row.event_id}:${row.kind}`}
								disableAdd
								disableSelect
								enableInternalActions={false}
								disablePagination
							/>
						</Section>

						<Section
							title="宿主网络状态"
							description="宿主机防火墙、WireGuard 与容器网络的只读检测结果。如状态异常，请按卡片说明在宿主机处理"
						>
							{health.isLoading ? (
								<Spinner size="small" />
							) : health.isError ? (
								<p className="color-fg-danger mb-0">
									宿主网络状态加载失败，请刷新页面后重试。
								</p>
							) : (
								<>
									<div style={FIELD_GRID}>
										{healthEntries.map((entry) => (
											<HealthCard key={entry.keyName} entry={entry} />
										))}
									</div>
									{healthData && healthData.notes.length > 0 && (
										<div className="mt-3">
											<div className="text-sm font-semibold mb-1">检测说明</div>
											<ul className="mb-0 pl-4 color-fg-muted text-sm">
												{healthData.notes.map((note) => (
													<li key={note}>{note}</li>
												))}
											</ul>
										</div>
									)}
								</>
							)}
						</Section>
					</>
				)}
			</Box>
		</div>
	);
}
