import { LockIcon, PackageIcon } from "@primer/octicons-react";
import {
	Box,
	Button,
	FormControl,
	Label,
	Spinner,
	TextInput,
	useConfirm,
} from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import type { AxiosError } from "axios";
import { type ChangeEvent, type ReactNode, useState } from "react";

import { adminApi } from "@/api";
import type { EventNetworkInfo } from "@floatctf/sdk";
import { useMsgBanner } from "@/components";
import {
	EMPTY_MANUAL_ALLOCATION_FORM,
	type ManualAllocationErrors,
	type ManualAllocationForm,
	hasErrors,
	validateManualAllocationForm,
} from "@/components/awd/networkForm";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/awd/$id/network")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

const QUERY_KEY = "awd-event-network";

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

/** 只读信息行：中文名称 + 原始字段名 + 取值。 */
function InfoRow({
	label,
	keyName,
	hint,
	value,
}: {
	label: string;
	keyName: string;
	hint?: string;
	value: string;
}) {
	return (
		<div className="flex items-start justify-between gap-4 border-b border-gray-100 py-2 last:border-0">
			<div>
				<div className="text-sm font-medium">{label}</div>
				<div className="font-mono text-xs color-fg-muted">{keyName}</div>
				{hint && (
					<div className="mt-1 max-w-[240px] text-xs color-fg-muted">
						{hint}
					</div>
				)}
			</div>
			<div className="text-right font-mono text-sm break-all">{value}</div>
		</div>
	);
}

/** 配置项：中文名称 + 原始字段名 + 说明 + 校验提示。 */
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

function RouteComponent() {
	const { id } = Route.useParams();
	const banner = useMsgBanner({});
	const confirmDialog = useConfirm();
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

	const onAllocated = (message: string) => () => {
		banner.showBanner("success", message);
		qc.invalidateQueries({ queryKey: [QUERY_KEY, id] });
	};

	const [manual, setManual] = useState<ManualAllocationForm>(
		EMPTY_MANUAL_ALLOCATION_FORM,
	);
	const manualErrors: ManualAllocationErrors =
		validateManualAllocationForm(manual);
	const setManualField =
		(key: keyof ManualAllocationForm) =>
		(event: ChangeEvent<HTMLInputElement>) => {
			setManual((prev) => ({ ...prev, [key]: event.target.value }));
		};

	const allocateAuto = useMutation({
		mutationFn: () => adminApi.awd.allocateEventNetwork(id, {}),
		onSuccess: onAllocated("已按自动方式分配赛事网络。"),
		onError: (e) => banner.showErrorBanner(e),
	});
	const allocateManual = useMutation({
		mutationFn: () =>
			adminApi.awd.allocateEventNetwork(id, {
				allocation_mode: "manual",
				gamebox_cidr: manual.gamebox_cidr.trim(),
				wireguard_cidr: manual.wireguard_cidr.trim(),
				wireguard_listen_port: manual.wireguard_listen_port.trim()
					? Number(manual.wireguard_listen_port)
					: undefined,
			}),
		onSuccess: onAllocated("已按手动指定的网段分配赛事网络。"),
		onError: (e) => banner.showErrorBanner(e),
	});
	const reallocate = useMutation({
		mutationFn: () => adminApi.awd.reallocateEventNetwork(id),
		onSuccess: onAllocated("已重新分配赛事网络。"),
		onError: (e) => banner.showErrorBanner(e),
	});

	const pending = allocateAuto.isPending || allocateManual.isPending;

	if (network.isLoading) {
		return <Spinner size="large" />;
	}

	const info = network.data;

	return (
		<div className="mt-3" style={{ maxWidth: 920 }}>
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
						<h3 className="m-0">赛事网络</h3>
						<p className="color-fg-muted mb-0 mt-1">
							本赛事使用的 GameBox 网段、WireGuard 网段与接入参数。
						</p>
					</div>
					<div className="d-flex flex-items-center gap-2">
						{network.isError ? (
							<Label variant="danger">加载失败</Label>
						) : info?.locked ? (
							<Label variant="danger">
								<LockIcon /> 已锁定
							</Label>
						) : info ? (
							<Label variant="success">已分配</Label>
						) : (
							<Label variant="accent">未分配</Label>
						)}
						{info && (
							<Label variant="default">
								{info.allocation_mode === "manual" ? "手动指定" : "自动分配"}
							</Label>
						)}
					</div>
				</div>

				{network.isError ? (
					<p className="color-fg-danger mt-4">
						网络信息加载失败，请刷新页面后重试。
					</p>
				) : !info ? (
					<>
						<Section
							title="自动分配"
							description="由平台从地址池中选取空闲网段，同时分配 GameBox 网段、WireGuard 网段与监听端口。"
						>
							<Box
								sx={{
									p: 3,
									bg: "canvas.subtle",
									borderRadius: 2,
								}}
							>
								<div className="d-flex flex-items-center gap-2">
									<Label variant="accent">推荐</Label>
									<span className="color-fg-muted text-sm">
										自动分配会避开平台已分配的网段与宿主机占用（Docker
										网络、路由）。
									</span>
								</div>
								<Box sx={{ mt: 3 }}>
									<Button
										variant="primary"
										leadingVisual={PackageIcon}
										disabled={pending}
										onClick={() => allocateAuto.mutate()}
									>
										{allocateAuto.isPending ? "分配中…" : "自动分配网络"}
									</Button>
								</Box>
							</Box>
						</Section>

						<Section
							title="手动指定网段"
							description="如自动分配结果不符合规划，可显式指定网段。网段必须合法且互不重叠，可以不在平台地址池内，但不得与平台已有分配或宿主机占用重叠；GameBox 网段不得比平台配置的队伍子网更小，且必须为 /16 或更大（AWD 运行时要求）。"
						>
							<div style={FIELD_GRID}>
								<Field
									label="GameBox 网段"
									keyName="gamebox_cidr"
									caption="必填。赛事容器使用的网段，例如 10.96.0.0/16；必须为 /16 或更大。"
									value={manual.gamebox_cidr}
									onChange={setManualField("gamebox_cidr")}
									placeholder="10.96.0.0/16"
									monospace
									error={
										manual.gamebox_cidr.trim()
											? manualErrors.gamebox_cidr
											: undefined
									}
								/>
								<Field
									label="WireGuard 网段"
									keyName="wireguard_cidr"
									caption="必填。队伍隧道使用的网段，例如 10.20.20.0/24。"
									value={manual.wireguard_cidr}
									onChange={setManualField("wireguard_cidr")}
									placeholder="10.112.0.0/16"
									monospace
									error={
										manual.wireguard_cidr.trim()
											? manualErrors.wireguard_cidr
											: undefined
									}
								/>
								<Field
									label="WireGuard 监听端口"
									keyName="wireguard_listen_port"
									caption="可选；需落在平台配置的端口范围内，留空则由平台自动分配。"
									value={manual.wireguard_listen_port}
									onChange={setManualField("wireguard_listen_port")}
									placeholder="51820"
									type="number"
									min={1}
									max={65535}
									error={
										manual.wireguard_listen_port.trim()
											? manualErrors.wireguard_listen_port
											: undefined
									}
								/>
							</div>
							<Box sx={{ mt: 3 }}>
								<Button
									leadingVisual={PackageIcon}
									disabled={hasErrors(manualErrors) || pending}
									onClick={() => allocateManual.mutate()}
								>
									{allocateManual.isPending ? "分配中…" : "按手动配置分配"}
								</Button>
							</Box>
						</Section>
					</>
				) : (
					<>
						<Section
							title="网络详情"
							description="赛事分配完成后固化的网络参数；已锁定的赛事不可修改。"
						>
							{info.locked && (
								<Box
									sx={{
										p: 3,
										mb: 3,
										bg: "attention.subtle",
										borderRadius: 2,
									}}
									className="text-sm"
								>
									赛事已部署，网络地址已锁定，不能重新分配。若部署失败，锁定会自动解除。
								</Box>
							)}
							<Box
								sx={{
									p: 3,
									border: "1px solid",
									borderColor: "border.default",
									borderRadius: 2,
								}}
							>
								<InfoRow
									label="GameBox 网段"
									keyName="gamebox_cidr"
									hint="赛事容器使用的网段。"
									value={info.gamebox_cidr}
								/>
								<InfoRow
									label="WireGuard 网段"
									keyName="wireguard_cidr"
									hint="队伍隧道使用的网段。"
									value={info.wireguard_cidr}
								/>
								<InfoRow
									label="基础设施子网"
									keyName="infrastructure_subnet"
									hint="FlagServer、JudgeServer 与平台控制面所在的子网，从 GameBox 网段中划分。"
									value={info.infrastructure_subnet}
								/>
								<InfoRow
									label="FlagServer 地址"
									keyName="flagserver_ip"
									hint="FlagServer 在基础设施子网中的地址。"
									value={info.flagserver_ip}
								/>
								<InfoRow
									label="JudgeServer 地址"
									keyName="judgeserver_ip"
									hint="JudgeServer 在基础设施子网中的地址。"
									value={info.judgeserver_ip}
								/>
								<InfoRow
									label="WireGuard 接口"
									keyName="wireguard_interface_name"
									hint="承载本赛事隧道的 WireGuard 接口名。"
									value={info.wireguard_interface_name}
								/>
								<InfoRow
									label="WireGuard 监听端口"
									keyName="wireguard_listen_port"
									hint="本赛事占用的 UDP 监听端口。"
									value={String(info.wireguard_listen_port)}
								/>
								<InfoRow
									label="Docker 网络"
									keyName="docker_network_name"
									hint="本赛事 GameBox 容器接入的 Docker 网络名。"
									value={info.docker_network_name}
								/>
							</Box>
						</Section>

						{!info.locked && (
							<Section
								title="重新分配"
								description="释放当前网段并重新从平台地址池中选取。仅未部署的赛事可执行。"
							>
								<Button
									variant="danger"
									leadingVisual={PackageIcon}
									disabled={reallocate.isPending}
									onClick={async () => {
										const ok = await confirmDialog({
											title: "确认重新分配网络？",
											content:
												"将释放当前网段并从平台地址池中重新选取，原有网段不再保留。",
											confirmButtonType: "danger",
										});
										if (ok) reallocate.mutate();
									}}
								>
									{reallocate.isPending ? "重新分配中…" : "重新分配网络"}
								</Button>
							</Section>
						)}
					</>
				)}
			</Box>
		</div>
	);
}
