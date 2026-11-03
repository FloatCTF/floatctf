/**
 * 版本与系统信息。
 *
 * 这一页回答运维最常问的三个问题：**跑的是哪个版本、宿主还撑得住吗、Docker 还好吗**。
 * 因此顺序是：版本 → 资源 → 磁盘 → 镜像与容器。
 *
 * 字节数一律走 `formatBytes` 本地格式化 —— 原始字节数（如 1.7e10）在
 * 界面上没有可读性，而"还剩多少"正是这一页唯一要传达的信息。
 */
import { Button, Card, Divider, Tag } from "animal-island-ui";
import {
	AlertTriangleIcon,
	ArrowDownIcon,
	ArrowUpIcon,
	CheckCircleIcon,
	GridIcon,
	RefreshIcon,
	MonitorIcon,
	TagIcon,
} from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniQuery } from "~/api/hooks";
import { PageHeader } from "~/app/shell";
import { EmptyState, ErrorState, LoadingState } from "~/components/states";
import { StatCard } from "~/components/resource";

/** 人类可读的字节数。 */
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

/** 使用率越高越红：超过 90% 是这一页真正需要被看见的信号。 */
function usageTag(percent: number | null | undefined) {
	if (percent === null || percent === undefined) return <Tag size="small" variant="outlined">未知</Tag>;
	const color = percent >= 90 ? "app-red" : percent >= 75 ? "app-yellow" : "app-green";
	return (
		<Tag size="small" variant="soft" color={color}>
			{percent.toFixed(1)}%
		</Tag>
	);
}

export function ConsoleVersion() {
	const runtime = useRuntime();
	const version = useUniQuery(
		["console", "version"],
		() => runtime.client.admin.system.version(),
		"加载版本",
	);
	const monitor = useUniQuery(
		["console", "monitor"],
		() => runtime.client.admin.system.monitor(),
		"加载系统信息",
	);

	if (version.isLoading || monitor.isLoading) return <LoadingState label="读取系统信息…" />;
	if (version.error) return <ErrorState error={version.error} onRetry={() => void version.refetch()} />;
	if (monitor.error) {
		return <ErrorState error={monitor.error} onRetry={() => void monitor.refetch()} />;
	}
	if (!monitor.data) {
		return (
			<ErrorState
				error={new Error("后端没有返回系统信息")}
				onRetry={() => void monitor.refetch()}
			/>
		);
	}

	const info = monitor.data;
	const disks = info.disks_info ?? [];
	const ifaces = info.network_interfaces ?? [];
	const docker = info.docker_info;

	return (
		<>
			<PageHeader
				title="版本与系统信息"
				description="当前构建、宿主资源与容器运行时状态。"
				extra={
					<Button
						size="small"
						icon={<RefreshIcon size={16} />}
						onClick={() => {
							void version.refetch();
							void monitor.refetch();
						}}
					>
						刷新
					</Button>
				}
			/>

			<Card>
				<div className="ai-row-between">
					<div className="ai-row" style={{ gap: 10 }}>
						<MonitorIcon size={20} />
						<div className="ai-col" style={{ gap: 2 }}>
							<span className="ai-strong" style={{ fontSize: 18 }}>
								{version.data ?? "未知版本"}
							</span>
							<span className="ai-muted" style={{ fontSize: 12 }}>
								{info.host_name ? `宿主 ${info.host_name}` : ""}
								{info.kernel_version ? ` ｜ 内核 ${info.kernel_version}` : ""}
								{info.os_version ? ` ｜ ${info.os_version}` : ""}
							</span>
						</div>
					</div>
					<Tag size="small" variant="soft" color="app-teal">
						平台 API
					</Tag>
				</div>
			</Card>

			<div className="ai-grid ai-grid-wide">
				<StatCard
					label="CPU 核心"
					value={info.nb_cpu ?? "—"}
					icon={<GridIcon size={18} />}
				/>
				<StatCard
					label="内存总量"
					value={formatBytes(info.total_memory)}
					hint={info.used_memory != null ? `已用 ${formatBytes(info.used_memory)}` : undefined}
				/>
				<StatCard
					label="运行时长"
					value={info.uptime != null ? `${Math.floor(info.uptime / 3600)} 小时` : "—"}
					icon={<CheckCircleIcon size={18} />}
				/>
				<StatCard
					label="运行中容器"
					value={docker?.running_container_count ?? "—"}
					hint={docker ? `镜像 ${docker.image_count} 个` : undefined}
				/>
			</div>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<span className="ai-strong" style={{ fontSize: 18 }}>
						磁盘
					</span>
					<Divider />
					{disks.length === 0 ? (
						<EmptyState title="没有磁盘信息" />
					) : (
						disks.map((d) => (
							<div key={`${d.name}-${d.mount_point}`} className="ai-col" style={{ gap: 4 }}>
								<div className="ai-row-between">
									<div className="ai-row" style={{ gap: 8 }}>
										<span className="ai-strong">{d.mount_point}</span>
										<Tag size="small" variant="outlined">
											{d.file_system}
										</Tag>
										{d.usage_percent >= 90 ? (
											<Tag size="small" variant="solid" color="app-red">
												<AlertTriangleIcon size={12} /> 空间紧张
											</Tag>
										) : null}
									</div>
									{usageTag(d.usage_percent)}
								</div>
								<span className="ai-muted" style={{ fontSize: 12 }}>
									已用 {formatBytes(d.used_space)} ／ 共 {formatBytes(d.total_space)}
									（可用 {formatBytes(d.available_space)}）
								</span>
							</div>
						))
					)}
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<span className="ai-strong" style={{ fontSize: 18 }}>
						网络接口
					</span>
					<Divider />
					{ifaces.length === 0 ? (
						<EmptyState title="没有网卡信息" />
					) : (
						ifaces.map((n) => (
							<div key={n.name} className="ai-row-between">
								<div className="ai-col" style={{ gap: 2, alignItems: "flex-start" }}>
									<span className="ai-strong ai-mono">{n.name}</span>
									<span className="ai-muted ai-mono" style={{ fontSize: 12 }}>
										{(n.ip_addresses ?? []).join(", ") || "（无地址）"}
									</span>
								</div>
								<div className="ai-col" style={{ gap: 2, alignItems: "flex-end" }}>
									<span className="ai-muted" style={{ fontSize: 12 }}>
										收 {formatBytes(n.received)} ／ 发 {formatBytes(n.transmitted)}
									</span>
									<span className="ai-row ai-muted" style={{ fontSize: 12, gap: 4 }}>
										速率
										<ArrowDownIcon size={12} />
										{formatBytes(n.recv_rate)}/s
										<ArrowUpIcon size={12} />
										{formatBytes(n.transmit_rate)}/s
									</span>
								</div>
							</div>
						))
					)}
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<div className="ai-row-between">
						<span className="ai-strong" style={{ fontSize: 18 }}>
							Docker 镜像
						</span>
						{docker ? (
							<Tag size="small" variant="outlined">
								<TagIcon size={12} /> {docker.image_count} 个 ／ {formatBytes(docker.total_disk)}
							</Tag>
						) : null}
					</div>
					<Divider />
					{!docker || (docker.images ?? []).length === 0 ? (
						<EmptyState
							title="没有镜像信息"
							description="Docker 不可用时这一块会是空的 —— 这不影响平台本身的其它功能。"
						/>
					) : (
						<div className="ai-scroll">
							<table style={{ width: "100%", borderCollapse: "collapse" }}>
								<thead>
									<tr>
										{["镜像", "标签", "大小"].map((h) => (
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
									{(docker.images ?? []).map((img) => (
										<tr key={img.id}>
											<td style={{ padding: "8px 12px" }} className="ai-mono">
												{img.id.slice(0, 12)}
											</td>
											<td style={{ padding: "8px 12px" }} className="ai-mono">
												{(img.repo_tags ?? []).join(", ") || "（无标签）"}
											</td>
											<td style={{ padding: "8px 12px" }} className="ai-muted">
												{formatBytes(img.size)}
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
