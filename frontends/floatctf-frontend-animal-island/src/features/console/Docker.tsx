/**
 * 容器运维。
 *
 * 三个分区：容器 / 镜像 / 网络。它们对应三类**真实副作用**：
 * 停容器会中断正在进行的比赛，删镜像会让之后无法拉起题目，
 * 删网络会切断靶机通信 —— 因此三类删除都要求输入确认词。
 *
 * 容器状态由 Docker 的 `state`（`running` / `exited` …）决定，不使用
 * 平台自己的实例状态：这是两个不同的真相层，混用会让运维按下按钮后
 * 看到一个和自己预期不符的结果。
 */
import { useState } from "react";
import {
	Button,
	Card,
	Input,
	Modal,
	Notification,
	Tabs,
	Tag,
	type TableColumn,
} from "animal-island-ui";
import {
	AlertTriangleIcon,
	PlayIcon,
	PlusIcon,
	RefreshIcon,
	StopIcon,
	TrashIcon,
} from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { Can, useCan } from "~/app/permissions";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { ResourcePage, StatCard } from "~/components/resource";
import type { FloatDockerContainer, ImageInfo, NetworkInfo } from "@floatctf/sdk";

function formatBytes(n: number | null | undefined): string {
	if (n === null || n === undefined || !Number.isFinite(n)) return "—";
	const units = ["B", "KiB", "MiB", "GiB", "TiB"];
	let v = n;
	let i = 0;
	while (v >= 1024 && i < units.length - 1) {
		v /= 1024;
		i += 1;
	}
	return `${v.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

/**
 * SDK 的 `FloatDockerContainer` **没有** `state` 字段（只有扁平的 `status` 文本，
 * 例如 "Up 2 hours" / "Exited (0) 3 minutes ago"），因此运行态只能从 `status`
 * 前缀派生。`ContainerInfo`（带 `state`/`names[]`）存在于 SDK 里，但
 * **不是** `fetchContainers` 的返回类型 —— 用它会被 tsc 拒绝。
 */
function isRunning(status: string | undefined): boolean {
	return /^up\b/i.test((status ?? "").trim());
}

function stateColor(status: string): "app-green" | "app-red" | "brown" {
	const s = status.toLowerCase();
	if (/^up\b/.test(s)) return "app-green";
	if (/^(exited|dead|created)\b/.test(s)) return "app-red";
	return "brown";
}

function ContainersTab() {
	const runtime = useRuntime();
	const confirm = useConfirm();
	const query = useUniQuery(
		["console", "docker", "containers"],
		() => runtime.client.admin.docker.fetchContainers(),
		"加载容器",
	);
	const stop = useUniMutation(
		(vars: string) => runtime.client.admin.docker.stopContainer(vars),
		"停止容器",
	);
	const start = useUniMutation(
		(vars: string) => runtime.client.admin.docker.startContainer(vars),
		"启动容器",
	);
	const remove = useUniMutation(
		(vars: string) => runtime.client.admin.docker.deleteContainer(vars),
		"删除容器",
	);

	// 「操作」列整列随权限显隐：只读用户看不到写入口，

	// 也不该看到一个永远为空的操作列（见下面的 columns 过滤）。

	const can = useCan();

	const columns: TableColumn<FloatDockerContainer>[] = [
		{
			title: "名称",
			dataIndex: "name",
			width: 240,
			render: (v) => <span className="ai-mono">{String(v)}</span>,
		},
		{
			title: "镜像",
			dataIndex: "image",
			width: 220,
			render: (v) => <span className="ai-mono" style={{ fontSize: 12 }}>{String(v)}</span>,
		},
		{
			title: "状态",
			dataIndex: "status",
			width: 200,
			render: (v) => (
				<Tag size="small" variant="soft" color={stateColor(String(v))}>
					{String(v) || "未知"}
				</Tag>
			),
		},
		{
			// `ports` 在这层 DTO 里是**字符串**（Docker 的原始端口文本），不是数组。
			title: "端口",
			dataIndex: "ports",
			render: (v) => (
				<span className="ai-muted ai-mono" style={{ fontSize: 12 }}>
					{String(v ?? "").trim() || "—"}
				</span>
			),
		},
		{
			title: "操作",
			width: 220,
			align: "right",
			render: (_v, row) => {
				const running = isRunning(row.status);
				return (
					<Can permission="docker.manage">
					<div className="ai-row" style={{ justifyContent: "flex-end", gap: 6 }}>
						{running ? (
							<Button
								size="small"
								icon={<StopIcon size={14} />}
								loading={stop.isPending}
								onClick={async () => {
									const ok = await confirm({
										title: "停止容器",
										consequence: `将停止 ${row.name || row.id.slice(0, 12)}。**如果它承载着正在进行的题目实例，选手会立刻失去连接**；比赛中的容器不会自动恢复。`,
										okText: "停止",
										danger: true,
									});
									if (!ok) return;
									await stop.mutateAsync(row.id);
									Notification.success({ message: "已停止" });
									void query.refetch();
								}}
							>
								停止
							</Button>
						) : (
							<Button
								size="small"
								type="primary"
								icon={<PlayIcon size={14} />}
								loading={start.isPending}
								onClick={async () => {
									await start.mutateAsync(row.id);
									Notification.success({ message: "已启动" });
									void query.refetch();
								}}
							>
								启动
							</Button>
						)}
						<Button
							size="small"
							type="text"
							danger
							icon={<TrashIcon size={14} />}
							onClick={async () => {
								const ok = await confirm({
									title: "删除容器",
									consequence:
										"容器会被**永久删除**（不只是停止）。如果它是某个选手正在用的实例，对方的连接与其中未保存的数据都会一起消失，且无法恢复。",
									okText: "永久删除",
									danger: true,
									confirmPhrase: "删除",
								});
								if (!ok) return;
								await remove.mutateAsync(row.id);
								Notification.success({ message: "已删除" });
								void query.refetch();
							}}
						>
							删除
						</Button>
					</div>
					</Can>
				);
			},
		},
	];

	return (
		<ResourcePage<FloatDockerContainer>
			columns={can("docker.manage") ? columns : columns.filter((c) => c.title !== "操作")}
			rows={query.data}
			isLoading={query.isLoading}
			error={query.error}
			refetch={() => void query.refetch()}
			rowKey="id"
			emptyTitle="没有容器"
			emptyDescription="当前宿主上没有 Docker 容器。"
			disablePagination
		/>
	);
}

function ImagesTab() {
	// 「操作」列随权限显隐（见 columns 使用处的过滤）。
	const can = useCan();
	const runtime = useRuntime();
	const confirm = useConfirm();
	const query = useUniQuery(
		["console", "docker", "images"],
		() => runtime.client.admin.docker.fetchImages(),
		"加载镜像",
	);
	const remove = useUniMutation(
		(vars: string) => runtime.client.admin.docker.deleteImage(vars),
		"删除镜像",
	);

	const columns: TableColumn<ImageInfo>[] = [
		{
			title: "标签",
			dataIndex: "repo_tags",
			width: 320,
			render: (v) => (
				<span className="ai-mono" style={{ fontSize: 12 }}>
					{(v as string[])?.join(", ") || "（无标签）"}
				</span>
			),
		},
		{
			title: "镜像 ID",
			dataIndex: "id",
			width: 160,
			render: (v) => <span className="ai-mono" style={{ fontSize: 12 }}>{String(v).slice(0, 12)}</span>,
		},
		{
			title: "大小",
			dataIndex: "size",
			width: 110,
			render: (v) => formatBytes(Number(v)),
		},
		{
			title: "操作",
			width: 110,
			align: "right",
			render: (_v, row) => (
				<Can permission="docker.manage">
				<Button
					size="small"
					type="text"
					danger
					icon={<TrashIcon size={14} />}
					onClick={async () => {
						const label = row.repo_tags?.[0] ?? row.id.slice(0, 12);
						const ok = await confirm({
							title: `删除镜像 ${label}`,
							consequence:
								"镜像会被删除。**之后拉起依赖它的题目/AWD 箱子会失败**，直到重新构建。如果还有容器在用，Docker 会拒绝删除。",
							okText: "删除镜像",
							danger: true,
							confirmPhrase: "删除",
						});
						if (!ok) return;
						await remove.mutateAsync(row.id);
						Notification.success({ message: "已删除" });
						void query.refetch();
					}}
				>
					删除
				</Button>
				</Can>
			),
		},
	];

	return (
		<ResourcePage<ImageInfo>
			columns={can("docker.manage") ? columns : columns.filter((c) => c.title !== "操作")}
			rows={query.data}
			isLoading={query.isLoading}
			error={query.error}
			refetch={() => void query.refetch()}
			rowKey="id"
			emptyTitle="没有镜像"
			disablePagination
		/>
	);
}

function NetworksTab() {
	// 「操作」列随权限显隐（见 columns 使用处的过滤）。
	const can = useCan();
	const runtime = useRuntime();
	const confirm = useConfirm();
	const [creating, setCreating] = useState(false);
	const [draft, setDraft] = useState({ name: "", subnet: "", gateway: "" });

	const query = useUniQuery(
		["console", "docker", "networks"],
		() => runtime.client.admin.docker.fetchNetworks(),
		"加载网络",
	);
	const create = useUniMutation(
		(vars: { name: string; subnet: string; gateway: string }) =>
			runtime.client.admin.docker.createNetwork(vars),
		"创建网络",
	);
	const remove = useUniMutation(
		(vars: string) => runtime.client.admin.docker.deleteNetwork(vars),
		"删除网络",
	);

	const columns: TableColumn<NetworkInfo>[] = [
		{ title: "名称", dataIndex: "name", width: 240 },
		{
			title: "驱动",
			dataIndex: "driver",
			width: 110,
			render: (v) => (
				<Tag size="small" variant="outlined">
					{String(v)}
				</Tag>
			),
		},
		{ title: "作用域", dataIndex: "scope", width: 100 },
		{ title: "IPAM 驱动", dataIndex: "ipam_driver", width: 120 },
		{
			title: "网络 ID",
			dataIndex: "id",
			width: 160,
			render: (v) => <span className="ai-mono" style={{ fontSize: 12 }}>{String(v).slice(0, 12)}</span>,
		},
		{
			title: "操作",
			width: 110,
			align: "right",
			render: (_v, row) => (
				<Can permission="docker.manage">
				<Button
					size="small"
					type="text"
					danger
					icon={<TrashIcon size={14} />}
					onClick={async () => {
						const ok = await confirm({
							title: `删除网络 ${row.name}`,
							consequence:
								"网络会被删除。**连接到它的容器会立刻失去网络通信**；如果是靶场网段，正在比赛的选手会断线。Docker 在仍有容器连接时会拒绝删除。",
							okText: "删除网络",
							danger: true,
							confirmPhrase: row.name,
						});
						if (!ok) return;
						await remove.mutateAsync(row.id);
						Notification.success({ message: "已删除" });
						void query.refetch();
					}}
				>
					删除
				</Button>
				</Can>
			),
		},
	];

	return (
		<div className="ai-col" style={{ gap: 12 }}>
			<Card>
				<div className="ai-row-between">
					<span className="ai-muted">
						手动创建的网络用于特殊拓扑。**平台自己的靶场网段由网段分配器管理**，不要在这里重复创建。
					</span>
					<Can permission="docker.manage">
						<Button
							type="primary"
							icon={<PlusIcon size={16} />}
							onClick={() => {
								setDraft({ name: "", subnet: "", gateway: "" });
								setCreating(true);
							}}
						>
							创建网络
						</Button>
					</Can>
				</div>
			</Card>

			<ResourcePage<NetworkInfo>
				columns={can("docker.manage") ? columns : columns.filter((c) => c.title !== "操作")}
				rows={query.data}
				isLoading={query.isLoading}
				error={query.error}
				refetch={() => void query.refetch()}
				rowKey="id"
				emptyTitle="没有网络"
				disablePagination
			/>

			<Modal
				open={creating}
				title="创建 Docker 网络"
				typewriter={false}
				onClose={() => setCreating(false)}
				footer={
					<>
						<Button onClick={() => setCreating(false)}>取消</Button>
						<Button
							type="primary"
							disabled={!draft.name.trim() || !draft.subnet.trim()}
							loading={create.isPending}
							onClick={async () => {
								await create.mutateAsync({
									name: draft.name.trim(),
									subnet: draft.subnet.trim(),
									gateway: draft.gateway.trim(),
								});
								Notification.success({ message: "网络已创建" });
								setCreating(false);
								void query.refetch();
							}}
						>
							创建
						</Button>
					</>
				}
			>
				<div className="ai-col" style={{ gap: 12 }}>
					<div className="ai-row" style={{ gap: 8 }}>
						<AlertTriangleIcon size={16} />
						<span className="ai-muted">
							网段**不能与已有网络重叠**，否则 Docker 会拒绝创建。
						</span>
					</div>
					<label className="ai-col" style={{ gap: 6 }}>
						<span>名称</span>
						<Input
							value={draft.name}
							placeholder="例如 fctf-custom-net"
							onChange={(e) => setDraft({ ...draft, name: e.target.value })}
						/>
					</label>
					<label className="ai-col" style={{ gap: 6 }}>
						<span>子网</span>
						<Input
							value={draft.subnet}
							placeholder="例如 10.99.0.0/24"
							onChange={(e) => setDraft({ ...draft, subnet: e.target.value })}
						/>
					</label>
					<label className="ai-col" style={{ gap: 6 }}>
						<span>网关</span>
						<Input
							value={draft.gateway}
							placeholder="例如 10.99.0.1"
							onChange={(e) => setDraft({ ...draft, gateway: e.target.value })}
						/>
					</label>
				</div>
			</Modal>
		</div>
	);
}

export function ConsoleDocker() {
	const runtime = useRuntime();
	const containers = useUniQuery(
		["console", "docker", "containers"],
		() => runtime.client.admin.docker.fetchContainers(),
		"加载容器",
	);
	const images = useUniQuery(
		["console", "docker", "images"],
		() => runtime.client.admin.docker.fetchImages(),
		"加载镜像",
	);
	const networks = useUniQuery(
		["console", "docker", "networks"],
		() => runtime.client.admin.docker.fetchNetworks(),
		"加载网络",
	);

	const running = (containers.data ?? []).filter((c) => isRunning(c.status)).length;

	return (
		<>
			<PageHeader
				title="容器运维"
				description="宿主的 Docker 资源。这里的操作会真实影响正在运行的题目与靶场。"
				extra={
					<Button
						size="small"
						icon={<RefreshIcon size={16} />}
						onClick={() => {
							void containers.refetch();
							void images.refetch();
							void networks.refetch();
						}}
					>
						刷新
					</Button>
				}
			/>

			<div className="ai-grid">
				<StatCard
					label="容器"
					value={containers.data?.length ?? "—"}
					hint={`运行中 ${running}`}
				/>
				<StatCard label="镜像" value={images.data?.length ?? "—"} />
				<StatCard label="网络" value={networks.data?.length ?? "—"} />
			</div>

			<Card>
				<Tabs
					aria-label="Docker 分区"
					defaultActiveKey="containers"
					items={[
						{ key: "containers", label: "容器", children: <ContainersTab /> },
						{ key: "images", label: "镜像", children: <ImagesTab /> },
						{ key: "networks", label: "网络", children: <NetworksTab /> },
					]}
				/>
			</Card>
		</>
	);
}
