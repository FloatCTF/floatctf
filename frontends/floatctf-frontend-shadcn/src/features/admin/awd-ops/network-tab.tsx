/**
 * 「网络」标签：赛事网络（Event Network）的查看 / 分配 / 重新分配。
 *
 * 已核实的后端语义：
 * - `GET .../awd/network` 在**未分配**时返回 404 → 本页把 404 当「未分配」，不是加载失败；
 * - `PUT .../awd/network`：已分配且未锁定 → 幂等 no-op；已锁定 → 拒绝。
 *   携带 gamebox_cidr / wireguard_cidr 任一即按 manual 分配（两个都需要），否则 automatic；
 * - `POST .../awd/network/reallocate`：仅未锁定且状态处于 Draft / Configuring 时可用
 *   （`assert_network_editable`），同事务里释放旧网段并写入新网段 + 新 WireGuard 端口。
 */

import { useState, type FormEvent } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Network, RefreshCw, Wand2 } from "lucide-react";

import type { EventNetworkInfo, NetworkAllocationRequest } from "@floatctf/sdk";

import { callVoid } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { Field, FormFooter, FormGrid, FormSheet } from "~/components/app/form";
import { CopyText, KeyValueList, MonoText, SectionCard } from "~/components/app/page";
import { EmptyBlock, QueryState, TableSkeleton } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";

import { useAwdEventNetwork, useAwdStatus } from "./queries";

const CIDR_PATTERN = /^\d{1,3}(\.\d{1,3}){3}\/\d{1,2}$/;

/** reallocate 的后端前提：未锁定且状态为 Draft / Configuring。 */
const NETWORK_EDITABLE_STATUSES = ["draft", "configuring"];

function ManualAllocationSheet({ eventId, onClose }: { eventId: string; onClose: () => void }) {
	const client = useClient();
	const queryClient = useQueryClient();
	const [gameboxCidr, setGameboxCidr] = useState("");
	const [wireguardCidr, setWireguardCidr] = useState("");
	const [listenPort, setListenPort] = useState("");
	const [errors, setErrors] = useState<Record<string, string | undefined>>({});

	const mutation = useMutation({
		mutationFn: (body: NetworkAllocationRequest) =>
			callVoid(client.awd.admin.allocateEventNetwork(eventId, body), "分配赛事网络"),
		onSuccess: () => {
			toast.success("赛事网络已分配", "部署时会锁定地址；请记得重新预检。");
			void queryClient.invalidateQueries({ queryKey: qk.awd.eventNetwork(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.awd.adminStatus(eventId) });
			onClose();
		},
		onError: (error) => toast.apiError("分配赛事网络失败", error),
	});

	const submit = (event: FormEvent) => {
		event.preventDefault();
		const nextErrors: Record<string, string | undefined> = {};
		if (!CIDR_PATTERN.test(gameboxCidr.trim())) nextErrors.gamebox_cidr = "需要形如 10.20.0.0/16 的 CIDR";
		if (!CIDR_PATTERN.test(wireguardCidr.trim())) {
			nextErrors.wireguard_cidr = "需要形如 10.30.0.0/16 的 CIDR";
		}
		if (listenPort.trim() !== "" && !/^\d+$/.test(listenPort.trim())) {
			nextErrors.listen_port = "必须是整数端口号";
		}
		setErrors(nextErrors);
		if (Object.values(nextErrors).some((value) => value !== undefined)) return;
		mutation.mutate({
			allocation_mode: "manual",
			gamebox_cidr: gameboxCidr.trim(),
			wireguard_cidr: wireguardCidr.trim(),
			...(listenPort.trim() === "" ? {} : { wireguard_listen_port: Number(listenPort) }),
		});
	};

	return (
		<FormSheet
			open
			onOpenChange={(open) => {
				if (!open) onClose();
			}}
			title="手动分配赛事网络"
			description="手动指定 GameBox 网段与 WireGuard 网段（两者都必须填写）；网段不得与平台池或宿主既有网段重叠。"
			width="lg"
			footer={
				<FormFooter
					onCancel={onClose}
					formId="awd-manual-network-form"
					submitLabel="分配"
					isPending={mutation.isPending}
				/>
			}
		>
			<form id="awd-manual-network-form" onSubmit={submit} className="space-y-4">
				<FormGrid columns={2}>
					<Field
						label="GameBox 网段（CIDR）"
						htmlFor="awd-network-gamebox-cidr"
						required
						error={errors.gamebox_cidr}
						hint="战队靶机所在的赛事网段"
					>
						<Input
							id="awd-network-gamebox-cidr"
							value={gameboxCidr}
							onChange={(event) => setGameboxCidr(event.target.value)}
							placeholder="10.20.0.0/16"
						/>
					</Field>
					<Field
						label="WireGuard 网段（CIDR）"
						htmlFor="awd-network-wireguard-cidr"
						required
						error={errors.wireguard_cidr}
						hint="选手隧道网段"
					>
						<Input
							id="awd-network-wireguard-cidr"
							value={wireguardCidr}
							onChange={(event) => setWireguardCidr(event.target.value)}
							placeholder="10.30.0.0/16"
						/>
					</Field>
					<Field
						label="WireGuard 监听端口"
						htmlFor="awd-network-listen-port"
						hint="留空则由平台在端口区间内自动分配"
						error={errors.listen_port}
					>
						<Input
							id="awd-network-listen-port"
							type="number"
							inputMode="numeric"
							value={listenPort}
							onChange={(event) => setListenPort(event.target.value)}
							placeholder="自动"
						/>
					</Field>
				</FormGrid>
			</form>
		</FormSheet>
	);
}

function NetworkDetail({ eventId, network }: { eventId: string; network: EventNetworkInfo }) {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const statusQuery = useAwdStatus(eventId);
	const status = statusQuery.data?.status ?? null;

	const reallocate = useMutation({
		mutationFn: () => callVoid(client.awd.admin.reallocateEventNetwork(eventId), "重新分配赛事网络"),
		onSuccess: () => {
			toast.success("赛事网络已重新分配", "网段与端口已更换；需要重新部署并重新预检。");
			void queryClient.invalidateQueries({ queryKey: qk.awd.eventNetwork(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.awd.adminStatus(eventId) });
		},
		onError: (error) => toast.apiError("重新分配赛事网络失败", error),
	});

	const editableStatus = status !== null && NETWORK_EDITABLE_STATUSES.includes(status);
	const canReallocate = !network.locked && editableStatus;

	const blockedReason = network.locked
		? "赛事网络已在部署时锁定（locked）：重新分配会被后端拒绝"
		: !editableStatus
			? `当前状态为「${status ?? "未知"}」：网络地址仅在草稿 / 配置中状态可改`
			: "从平台池重新分配 GameBox / WireGuard 网段与 WireGuard 端口";

	return (
		<SectionCard
			title="赛事网络分配"
			description="赛事专属的 GameBox 网段、WireGuard 网段与基础设施地址。"
			actions={
				<Button
					variant="destructive"
					size="sm"
					disabled={!canReallocate || reallocate.isPending}
					title={blockedReason}
					onClick={async () => {
						const ok = await confirm({
							title: "重新分配赛事网络？（高危）",
							description: "会更换本赛事的网段与 WireGuard 端口。",
							consequences: [
								"释放当前 GameBox / WireGuard 网段，并从平台池重新分配新网段与新监听端口",
								"已部署的容器与 Docker 网络不会自动迁移到新网段，需要重新部署才会生效",
								"旧网段的连接与 WireGuard 对端不再属于本赛事；重新分配后必须重新预检",
								"仅在网络未锁定（草稿 / 配置中）时可用，部署会锁定地址",
							],
							tone: "danger",
							confirmText: "重新分配",
							confirmPhrase: "reallocate",
						});
						if (ok) reallocate.mutate();
					}}
				>
					<RefreshCw /> 重新分配
				</Button>
			}
		>
			<div className="space-y-4">
				<div className="flex flex-wrap items-center gap-2">
					<TonePill tone={network.locked ? "warning" : "info"}>
						{network.locked ? "地址已锁定" : "地址可改"}
					</TonePill>
					<TonePill tone="neutral">
						分配方式：
						{network.allocation_mode === "manual" ? "手动" : "自动"}
					</TonePill>
					{!canReallocate ? (
						<span className="text-xs text-muted-foreground">{blockedReason}</span>
					) : null}
				</div>
				<KeyValueList
					columns={2}
					items={[
						{ key: "GameBox 网段", value: <MonoText>{network.gamebox_cidr}</MonoText> },
						{ key: "WireGuard 网段", value: <MonoText>{network.wireguard_cidr}</MonoText> },
						{
							key: "基础设施子网",
							value: <MonoText>{network.infrastructure_subnet}</MonoText>,
						},
						{ key: "FlagServer IP", value: <MonoText>{network.flagserver_ip}</MonoText> },
						{ key: "JudgeServer IP", value: <MonoText>{network.judgeserver_ip}</MonoText> },
						{ key: "Docker 网络", value: <CopyText value={network.docker_network_name} /> },
						{
							key: "WireGuard 接口",
							value: <MonoText>{network.wireguard_interface_name}</MonoText>,
						},
						{
							key: "WireGuard 端口",
							value: <MonoText>{network.wireguard_listen_port}</MonoText>,
						},
					]}
				/>
			</div>
		</SectionCard>
	);
}

export function AwdNetworkTab({ eventId }: { eventId: string }) {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const networkQuery = useAwdEventNetwork(eventId);
	const [manualOpen, setManualOpen] = useState(false);

	const automatic = useMutation({
		mutationFn: () =>
			callVoid(
				client.awd.admin.allocateEventNetwork(eventId, { allocation_mode: "automatic" }),
				"自动分配赛事网络",
			),
		onSuccess: () => {
			toast.success("赛事网络已自动分配", "部署时会锁定地址；请记得运行预检。");
			void queryClient.invalidateQueries({ queryKey: qk.awd.eventNetwork(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.awd.adminStatus(eventId) });
		},
		onError: (error) => toast.apiError("自动分配赛事网络失败", error),
	});

	return (
		<>
			<QueryState
				query={networkQuery}
				skeleton={<TableSkeleton rows={5} columns={2} />}
				isEmpty={() => networkQuery.data === null}
				empty={
				<EmptyBlock
					title="赛事网络尚未分配"
					description="部署前必须先分配赛事网络：自动分配会从平台池取网段，手动分配可指定 CIDR。"
					icon={<Network className="size-5" />}
					action={
						<div className="flex flex-wrap items-center justify-center gap-2">
							<Button
								size="sm"
								disabled={automatic.isPending}
								onClick={async () => {
									const ok = await confirm({
										title: "自动分配赛事网络？",
										description: "从平台网络池为本赛事分配网段。",
										consequences: [
											"分配 GameBox 网段、WireGuard 网段、基础设施子网与 WireGuard 监听端口",
											"分配不会创建容器；容器在「部署」时创建",
											"部署时地址会被锁定；未锁定时重复分配是幂等的",
										],
										confirmText: "自动分配",
									});
									if (ok) automatic.mutate();
								}}
							>
								<Wand2 /> 自动分配
							</Button>
							<Button variant="outline" size="sm" onClick={() => setManualOpen(true)}>
								手动分配
							</Button>
						</div>
					}
				/>
			}
		>
			{(network) =>
				network ? (
					<NetworkDetail eventId={eventId} network={network} />
				) : null
			}
			</QueryState>

			{/* 表单挂在 QueryState 之外：未分配时 QueryState 渲染的是空态而不是 children。 */}
			{manualOpen ? (
				<ManualAllocationSheet eventId={eventId} onClose={() => setManualOpen(false)} />
			) : null}
		</>
	);
}
