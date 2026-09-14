import { Button, FormControl, Label, Spinner, TextInput } from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive } from "ahooks";
import { useEffect, useRef } from "react";

import { adminApi } from "@/api";
import type {
	PlatformNetworkAllocation,
	PlatformNetworkHealth,
	PlatformNetworkSettings,
} from "@/api/awd";
import type { QueryParams, UniResponse } from "@/api/axios";
import { GenericTable, useMsgBanner } from "@/components";
import { DatetimeToShow } from "@/util";
import { AdminRouteGuard } from "../route";

export const Route = createFileRoute("/admin/awd/network")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

const SETTINGS_KEY = "AWDPlatformNetworkSettings";
const HEALTH_KEY = "AWDPlatformNetworkHealth";
const ALLOCATIONS_KEY = "AWDPlatformNetworkAllocations";

/** Host 观测只读卡片（灰色信息样式，无操作按钮）。 */
function HealthCard({
	label,
	value,
}: {
	label: string;
	value?: string | null;
}) {
	return (
		<div className="rounded border border-gray-200 bg-gray-50 p-3">
			<div className="text-xs font-semibold text-gray-500 uppercase">
				{label}
			</div>
			<div className="mt-1 font-mono text-sm break-all">{value ?? "-"}</div>
		</div>
	);
}

/** 容量预览小项。 */
function CapacityItem({ label, value }: { label: string; value: number }) {
	return (
		<div>
			<div className="text-xs text-gray-500">{label}</div>
			<div className="font-mono text-sm font-semibold">{value}</div>
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

	const onSaved = (msg: string) => () => {
		banner.showBanner("success", msg);
		qc.invalidateQueries({ queryKey: [SETTINGS_KEY] });
	};

	// ── 表单状态（初次加载后从 GET 回填）──
	const form = useReactive({
		gamebox_pool: "",
		gamebox_event_prefix: 0,
		gamebox_team_prefix: 0,
		wireguard_pool: "",
		wireguard_event_prefix: 0,
		wireguard_team_prefix: 0,
		wireguard_public_endpoint: "",
		wireguard_port_min: 0,
		wireguard_port_max: 0,
	});
	// 只从首次 GET 回填表单，之后查询刷新（窗口聚焦等）不覆盖用户未保存的编辑
	const initialized = useRef(false);
	useEffect(() => {
		const s = settings.data?.data;
		if (s && !initialized.current) {
			initialized.current = true;
			Object.assign(form, {
				gamebox_pool: s.gamebox_pool,
				gamebox_event_prefix: s.gamebox_event_prefix,
				gamebox_team_prefix: s.gamebox_team_prefix,
				wireguard_pool: s.wireguard_pool,
				wireguard_event_prefix: s.wireguard_event_prefix,
				wireguard_team_prefix: s.wireguard_team_prefix,
				wireguard_public_endpoint: s.wireguard_public_endpoint ?? "",
				wireguard_port_min: s.wireguard_port_min,
				wireguard_port_max: s.wireguard_port_max,
			});
		}
	}, [settings.data]);

	// §5：Address Pools 保存（只 PATCH pool / prefix 字段）
	const savePools = useMutation({
		mutationFn: () =>
			adminApi.awd.updatePlatformNetwork({
				gamebox_pool: form.gamebox_pool.trim(),
				gamebox_event_prefix: form.gamebox_event_prefix,
				gamebox_team_prefix: form.gamebox_team_prefix,
				wireguard_pool: form.wireguard_pool.trim(),
				wireguard_event_prefix: form.wireguard_event_prefix,
				wireguard_team_prefix: form.wireguard_team_prefix,
			}),
		onSuccess: onSaved("Address pools saved"),
		onError: banner.showErrorBanner,
	});

	// §6：WireGuard Settings 保存
	const saveWireguard = useMutation({
		mutationFn: () =>
			adminApi.awd.updatePlatformNetwork({
				wireguard_public_endpoint:
					form.wireguard_public_endpoint.trim() || null,
				wireguard_port_min: form.wireguard_port_min,
				wireguard_port_max: form.wireguard_port_max,
			}),
		onSuccess: onSaved("WireGuard settings saved"),
		onError: banner.showErrorBanner,
	});

	// ── §7：Current Allocations（只读，释放走 Event lifecycle）──
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
			header: "Event",
			field: "event_id",
			renderCell: (row: PlatformNetworkAllocation) => (
				<span>
					{row.event_title ?? row.event_id}
					{row.event_title && (
						<span className="text-gray-500"> ({row.event_id})</span>
					)}
				</span>
			),
		},
		{
			accessorKey: "kind",
			header: "Kind",
			field: "kind",
			renderCell: (row: PlatformNetworkAllocation) => (
				<span className="font-mono">{row.kind}</span>
			),
		},
		{
			accessorKey: "cidr",
			header: "CIDR",
			field: "cidr",
			renderCell: (row: PlatformNetworkAllocation) => (
				<span className="font-mono">{row.cidr}</span>
			),
		},
		{
			accessorKey: "allocated_at",
			header: "Allocated At",
			field: "allocated_at",
			renderCell: (row: PlatformNetworkAllocation) => (
				<span>{DatetimeToShow(row.allocated_at)}</span>
			),
		},
		{
			accessorKey: "released_at",
			header: "Released At",
			field: "released_at",
			renderCell: (row: PlatformNetworkAllocation) => (
				<span>{DatetimeToShow(row.released_at)}</span>
			),
		},
		{
			accessorKey: "active",
			header: "Active",
			field: "active",
			renderCell: (row: PlatformNetworkAllocation) => (
				<Label variant={row.active ? "success" : "default"}>
					{row.active ? "active" : "released"}
				</Label>
			),
		},
	];

	const healthData: PlatformNetworkHealth | undefined = health.data?.data;
	const healthItems = healthData
		? [
				{ label: "nftables", value: healthData.nftables },
				{ label: "wireguard", value: healthData.wireguard },
				{ label: "docker", value: healthData.docker },
				{
					label: "firewall_runtime",
					value: healthData.firewall_runtime,
				},
				{ label: "floatctf_table", value: healthData.floatctf_table },
				{
					label: "docker_firewall_backend",
					value: healthData.docker_firewall_backend,
				},
				{ label: "firewalld", value: healthData.firewalld },
				{
					label: "ipv4_forwarding",
					value: healthData.ipv4_forwarding,
				},
				{ label: "ipv6_policy", value: healthData.ipv6_policy },
			]
		: [];

	const s: PlatformNetworkSettings | undefined = settings.data?.data;
	const numField = (v: number) => String(v);

	return (
		<div className="m-2 flex flex-col gap-6">
			<banner.BannerComponent />

			{/* §4.1 Host Status —— 纯观测，无任何操作按钮 */}
			<section>
				<div className="mb-2 flex items-center gap-2">
					<h4 className="font-bold">Host Status</h4>
					<Label variant="default">read-only</Label>
				</div>
				{health.isLoading ? (
					<Spinner size="small" />
				) : health.isError ? (
					<div className="text-sm text-red-600">Failed to load host status</div>
				) : (
					<div className="flex flex-col gap-3">
						<div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-5 gap-2">
							{healthItems.map((item) => (
								<HealthCard
									key={item.label}
									label={item.label}
									value={item.value}
								/>
							))}
							<div className="rounded border border-gray-200 bg-gray-50 p-3">
								<div className="text-xs font-semibold text-gray-500 uppercase">
									capability_supported
								</div>
								<div className="mt-1">
									<Label
										variant={
											healthData?.capability_supported ? "success" : "danger"
										}
									>
										{healthData?.capability_supported
											? "supported"
											: "unsupported"}
									</Label>
								</div>
							</div>
						</div>
						{healthData && healthData.notes.length > 0 && (
							<div className="rounded border border-gray-200 bg-gray-50 p-3 text-sm">
								<div className="text-xs font-semibold text-gray-500 uppercase mb-1">
									Notes
								</div>
								<ul className="list-disc list-inside text-gray-700">
									{healthData.notes.map((n) => (
										<li key={n}>{n}</li>
									))}
								</ul>
							</div>
						)}
						<p className="text-xs text-gray-500">
							只读观测（§4.1）：页面不提供任何修改宿主防火墙的操作。
						</p>
					</div>
				)}
			</section>

			{/* §5 Address Pools + 容量预览 */}
			<section>
				<h4 className="font-bold mb-2">Address Pools</h4>
				{settings.isLoading ? (
					<Spinner size="small" />
				) : (
					<div className="flex flex-col gap-4">
						<div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
							<FormControl>
								<FormControl.Label>gamebox_pool</FormControl.Label>
								<TextInput
									value={form.gamebox_pool}
									onChange={(e) => {
										form.gamebox_pool = e.target.value;
									}}
									placeholder="10.10.0.0/16"
									monospace
								/>
							</FormControl>
							<FormControl>
								<FormControl.Label>gamebox_event_prefix</FormControl.Label>
								<TextInput
									value={numField(form.gamebox_event_prefix)}
									onChange={(e) => {
										form.gamebox_event_prefix = Number(e.target.value);
									}}
									placeholder="20"
								/>
							</FormControl>
							<FormControl>
								<FormControl.Label>gamebox_team_prefix</FormControl.Label>
								<TextInput
									value={numField(form.gamebox_team_prefix)}
									onChange={(e) => {
										form.gamebox_team_prefix = Number(e.target.value);
									}}
									placeholder="24"
								/>
							</FormControl>
							<FormControl>
								<FormControl.Label>wireguard_pool</FormControl.Label>
								<TextInput
									value={form.wireguard_pool}
									onChange={(e) => {
										form.wireguard_pool = e.target.value;
									}}
									placeholder="10.20.0.0/16"
									monospace
								/>
							</FormControl>
							<FormControl>
								<FormControl.Label>wireguard_event_prefix</FormControl.Label>
								<TextInput
									value={numField(form.wireguard_event_prefix)}
									onChange={(e) => {
										form.wireguard_event_prefix = Number(e.target.value);
									}}
									placeholder="20"
								/>
							</FormControl>
							<FormControl>
								<FormControl.Label>wireguard_team_prefix</FormControl.Label>
								<TextInput
									value={numField(form.wireguard_team_prefix)}
									onChange={(e) => {
										form.wireguard_team_prefix = Number(e.target.value);
									}}
									placeholder="24"
								/>
							</FormControl>
						</div>
						<div>
							<Button
								variant="primary"
								disabled={savePools.isPending}
								onClick={() => savePools.mutate()}
							>
								{savePools.isPending ? "Saving…" : "Save Pools"}
							</Button>
						</div>
						{/* §67 容量预览 */}
						{s && (
							<div className="rounded border border-gray-200 bg-gray-50 p-3">
								<div className="text-xs font-semibold text-gray-500 uppercase mb-2">
									Capacity Preview
								</div>
								<div className="grid grid-cols-2 md:grid-cols-3 gap-2">
									<CapacityItem
										label="GameBox events"
										value={s.gamebox_event_capacity}
									/>
									<CapacityItem
										label="GameBox teams / event"
										value={s.gamebox_team_capacity_per_event}
									/>
									<CapacityItem
										label="GameBox hosts / team"
										value={s.gamebox_hosts_per_team}
									/>
									<CapacityItem
										label="WireGuard events"
										value={s.wireguard_event_capacity}
									/>
									<CapacityItem
										label="WireGuard teams / event"
										value={s.wireguard_team_capacity_per_event}
									/>
									<CapacityItem
										label="WireGuard ports"
										value={s.wireguard_port_capacity}
									/>
								</div>
								<div className="mt-2 text-xs text-gray-500">
									Last updated: {DatetimeToShow(s.updated_at)}
									{" — "}
									保存仅影响 future allocations（§31/§32）。
								</div>
							</div>
						)}
					</div>
				)}
			</section>

			{/* §6 WireGuard Settings */}
			<section>
				<h4 className="font-bold mb-2">WireGuard Settings</h4>
				<div className="flex flex-col gap-4">
					<div className="grid grid-cols-1 md:grid-cols-3 gap-3">
						<FormControl>
							<FormControl.Label>wireguard_public_endpoint</FormControl.Label>
							<TextInput
								value={form.wireguard_public_endpoint}
								onChange={(e) => {
									form.wireguard_public_endpoint = e.target.value;
								}}
								placeholder="vpn.example.com:51820"
								monospace
							/>
							<FormControl.Caption>
								玩家 WireGuard 配置里的公开端点 （host:port）
							</FormControl.Caption>
						</FormControl>
						<FormControl>
							<FormControl.Label>wireguard_port_min</FormControl.Label>
							<TextInput
								value={numField(form.wireguard_port_min)}
								onChange={(e) => {
									form.wireguard_port_min = Number(e.target.value);
								}}
								placeholder="51820"
							/>
						</FormControl>
						<FormControl>
							<FormControl.Label>wireguard_port_max</FormControl.Label>
							<TextInput
								value={numField(form.wireguard_port_max)}
								onChange={(e) => {
									form.wireguard_port_max = Number(e.target.value);
								}}
								placeholder="51830"
							/>
						</FormControl>
					</div>
					<div>
						<Button
							variant="primary"
							disabled={saveWireguard.isPending}
							onClick={() => saveWireguard.mutate()}
						>
							{saveWireguard.isPending ? "Saving…" : "Save WireGuard Settings"}
						</Button>
					</div>
				</div>
			</section>

			{/* §7 Current Allocations —— 只读，不提供删除（释放走 Event lifecycle） */}
			<section>
				<h4 className="font-bold mb-2">Current Allocations</h4>
				<GenericTable
					subject={ALLOCATIONS_KEY}
					columns={allocationColumns}
					queryFn={allocationQueryFn}
					getRowId={(row) => `${row.event_id}:${row.kind}`}
					disableAdd
					disableSelect
					enableInternalActions={false}
					disablePagination
					subtitle="只读账本：CIDR 释放必须走 Event lifecycle（不可在此删除）"
				/>
			</section>
		</div>
	);
}
