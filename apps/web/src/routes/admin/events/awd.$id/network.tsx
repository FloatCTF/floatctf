import { LockIcon, PackageIcon } from "@primer/octicons-react";
import { Button, FormControl, Label, Spinner, TextInput } from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import type { AxiosError } from "axios";
import { useState } from "react";

import { adminApi } from "@/api";
import type { EventNetworkInfo } from "@/api/awd";
import { useMsgBanner } from "@/components";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/awd/$id/network")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

const QUERY_KEY = "awd-event-network";

/** 只读信息行（已分配视图）。 */
function InfoRow({ label, value }: { label: string; value: string }) {
	return (
		<div className="flex items-baseline justify-between gap-4 border-b border-gray-100 py-2 last:border-0">
			<span className="text-sm text-gray-500">{label}</span>
			<span className="font-mono text-sm break-all">{value}</span>
		</div>
	);
}

function RouteComponent() {
	const { id } = Route.useParams();
	const banner = useMsgBanner({});
	const qc = useQueryClient();

	// 未分配时后端返回 404（data=null）→ 归一化为 null，区分“未分配”与真实错误
	const network = useQuery({
		queryKey: [QUERY_KEY, id],
		queryFn: async (): Promise<EventNetworkInfo | null> => {
			try {
				const res = await adminApi.awd.getEventNetwork(id);
				return res.data ?? null;
			} catch (e) {
				const status = (e as AxiosError)?.response?.status;
				if (status === 404) return null;
				throw e;
			}
		},
		retry: false,
		staleTime: 30_000,
	});

	const onAllocated = (msg: string) => () => {
		banner.showBanner("success", msg);
		qc.invalidateQueries({ queryKey: [QUERY_KEY, id] });
	};

	// 未分配视图：Automatic 默认 + Advanced 手动（§23/§24）
	const [showAdvanced, setShowAdvanced] = useState(false);
	const [manual, setManual] = useState({
		gamebox_cidr: "",
		wireguard_cidr: "",
		wireguard_listen_port: "",
	});
	const manualReady =
		manual.gamebox_cidr.trim().length > 0 &&
		manual.wireguard_cidr.trim().length > 0;

	const allocateAuto = useMutation({
		mutationFn: () => adminApi.awd.allocateEventNetwork(id, {}),
		onSuccess: onAllocated("Network allocated (automatic)"),
		onError: banner.showErrorBanner,
	});
	const allocateManual = useMutation({
		mutationFn: () =>
			adminApi.awd.allocateEventNetwork(id, {
				allocation_mode: "manual",
				gamebox_cidr: manual.gamebox_cidr.trim(),
				wireguard_cidr: manual.wireguard_cidr.trim(),
				wireguard_listen_port: manual.wireguard_listen_port
					? Number(manual.wireguard_listen_port)
					: undefined,
			}),
		onSuccess: onAllocated("Network allocated (manual)"),
		onError: banner.showErrorBanner,
	});

	// 已分配未锁定：§33/§93 Reallocate
	const reallocate = useMutation({
		mutationFn: () => adminApi.awd.reallocateEventNetwork(id),
		onSuccess: onAllocated("Network reallocated"),
		onError: banner.showErrorBanner,
	});

	const pending = allocateAuto.isPending || allocateManual.isPending;

	if (network.isLoading) {
		return <Spinner size="large" />;
	}
	if (network.isError) {
		return <div>Error loading network</div>;
	}

	const info = network.data;

	// ── 未分配：Automatic 默认 + Advanced 手动 ──
	if (!info) {
		return (
			<div className="m-2 flex flex-col gap-4">
				<banner.BannerComponent />
				<section>
					<h4 className="font-bold mb-2">Network Allocation</h4>
					<div className="rounded border border-gray-200 bg-gray-50 p-4">
						<div className="flex flex-col gap-3">
							<div className="flex items-center gap-2">
								<Label variant="accent">automatic</Label>
								<span className="text-sm text-gray-600">
									默认分配模式：从平台 pool 自动挑选 GameBox / WireGuard CIDR
								</span>
							</div>
							<div>
								<Button
									variant="primary"
									leadingVisual={PackageIcon}
									disabled={pending}
									onClick={() => allocateAuto.mutate()}
								>
									{pending ? "Allocating…" : "Allocate Network"}
								</Button>
							</div>
							<Button
								variant="invisible"
								size="small"
								onClick={() => setShowAdvanced(!showAdvanced)}
							>
								{showAdvanced ? "Hide Advanced" : "Advanced (manual CIDR)"}
							</Button>
							{showAdvanced && (
								<div className="grid grid-cols-1 md:grid-cols-3 gap-3 pt-2 border-t border-gray-200">
									<FormControl>
										<FormControl.Label>gamebox_cidr</FormControl.Label>
										<TextInput
											value={manual.gamebox_cidr}
											onChange={(e) => {
												manual.gamebox_cidr = e.target.value;
											}}
											placeholder="10.10.20.0/24"
											monospace
										/>
									</FormControl>
									<FormControl>
										<FormControl.Label>wireguard_cidr</FormControl.Label>
										<TextInput
											value={manual.wireguard_cidr}
											onChange={(e) => {
												manual.wireguard_cidr = e.target.value;
											}}
											placeholder="10.20.20.0/24"
											monospace
										/>
									</FormControl>
									<FormControl>
										<FormControl.Label>
											wireguard_listen_port (可选)
										</FormControl.Label>
										<TextInput
											value={manual.wireguard_listen_port}
											onChange={(e) => {
												manual.wireguard_listen_port = e.target.value;
											}}
											placeholder="51820"
										/>
									</FormControl>
									<div className="md:col-span-3">
										<Button
											leadingVisual={PackageIcon}
											disabled={!manualReady || pending}
											onClick={() => allocateManual.mutate()}
										>
											{allocateManual.isPending
												? "Allocating…"
												: "Allocate (Manual)"}
										</Button>
									</div>
								</div>
							)}
						</div>
					</div>
				</section>
			</div>
		);
	}

	// ── 已分配（未锁定 / 已锁定）──
	const isLocked = info.locked;
	return (
		<div className="m-2 flex flex-col gap-4">
			<banner.BannerComponent />
			<section>
				<div className="mb-2 flex items-center gap-2">
					<h4 className="font-bold">Network</h4>
					{isLocked ? (
						<Label variant="danger">
							<LockIcon /> locked
						</Label>
					) : (
						<Label variant="success">allocated</Label>
					)}
					<Label variant="default">{info.allocation_mode}</Label>
				</div>

				{isLocked && (
					<div className="mb-3 rounded border border-yellow-200 bg-yellow-50 p-3 text-sm text-yellow-800">
						🔒 Network locked after deployment：地址已固化， 不可 reallocate
						或修改。
					</div>
				)}

				<div className="rounded border border-gray-200 bg-gray-50 p-4">
					<InfoRow label="GameBox CIDR" value={info.gamebox_cidr} />
					<InfoRow label="WireGuard CIDR" value={info.wireguard_cidr} />
					<InfoRow
						label="Infrastructure Subnet"
						value={info.infrastructure_subnet}
					/>
					<InfoRow label="FlagServer IP" value={info.flagserver_ip} />
					<InfoRow label="JudgeServer IP" value={info.judgeserver_ip} />
					<InfoRow
						label="WireGuard Interface"
						value={info.wireguard_interface_name}
					/>
					<InfoRow
						label="WireGuard Listen Port"
						value={String(info.wireguard_listen_port)}
					/>
					<InfoRow label="Docker Network" value={info.docker_network_name} />
				</div>

				{!isLocked && (
					<div className="mt-3">
						<Button
							leadingVisual={PackageIcon}
							disabled={reallocate.isPending}
							onClick={() => {
								if (
									window.confirm(
										"确认重新分配该 Event 的网络？\n将释放当前 CIDR 并重新从平台 pool 挑选（仅未锁定时可操作）。",
									)
								) {
									reallocate.mutate();
								}
							}}
						>
							{reallocate.isPending ? "Reallocating…" : "Reallocate"}
						</Button>
					</div>
				)}
			</section>
		</div>
	);
}
