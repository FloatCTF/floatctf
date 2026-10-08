/**
 * 管理端总览（`/admin`）—— 平台聚合状态 + 系统监控 + 平台版本。
 *
 * 三个真实接口：
 * - `client.admin.dashboard.summary()` → 聚合统计 / 需要关注 / 赛事 / 动态（60s 轮询）；
 * - `client.admin.system.monitor()`   → 宿主 CPU·内存·磁盘·网卡·Docker（60s 轮询）；
 * - `client.admin.system.version()`   → 平台版本。
 *
 * `DashboardSummary` 的子结构在 SDK 里是内联匿名类型，这里用索引类型取用，
 * 不额外复制一份 DTO（避免与 SDK 漂移）。
 */

import type { ReactNode } from "react";
import { Link } from "react-router";
import { useQuery } from "@tanstack/react-query";
import {
	AlertTriangle,
	Boxes,
	CalendarDays,
	Container,
	Cpu,
	HardDrive,
	Megaphone,
	MessagesSquare,
	Puzzle,
	RefreshCw,
	ServerCog,
	Swords,
	Thermometer,
	Users,
} from "lucide-react";

import { call } from "~/api/call";
import { useAppQueryClient, useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { BooleanPill, TonePill } from "~/components/app/badges";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import {
	CopyText,
	MonoText,
	PageBody,
	PageHeader,
	SectionCard,
	StatCard,
} from "~/components/app/page";
import { EmptyBlock, LoadingBlock, QueryState } from "~/components/app/states";
import { UserAvatar } from "~/components/app/user-cell";
import { Button } from "~/components/ui/button";
import { Progress } from "~/components/ui/progress";
import {
	EVENT_FAMILY_LABEL,
	EVENT_PURPOSE_LABEL,
	EVENT_STATUS_LABEL,
	EVENT_STATUS_TONE,
	PARTICIPANT_MODE_LABEL,
	computeEventStatus,
} from "~/lib/event-status";
import {
	formatBytes,
	formatDuration,
	formatInt,
	formatRange,
	formatRelative,
} from "~/lib/format";
import { useDocumentTitle, useNow } from "~/lib/hooks";
import type {
	DashboardSummary,
	DiskInformation,
	NetworkInterfaceInfo,
	SystemInformation,
} from "@floatctf/sdk";

type SummaryEvent = DashboardSummary["events"][number];
type MonitorQuery = {
	isPending: boolean;
	isError: boolean;
	error: unknown;
	data: SystemInformation | undefined;
	refetch: () => void;
};

function percent(used: number, total: number): number {
	if (!Number.isFinite(used) || !Number.isFinite(total) || total <= 0) return 0;
	return Math.min(100, Math.max(0, (used / total) * 100));
}

/** sysinfo 读不到传感器时返回 0；0 不代表「0 度」，如实显示为未知。 */
function temperatureText(value: number): string {
	if (!Number.isFinite(value) || value <= 0) return "—";
	return `${value.toFixed(1)} °C`;
}

/** 磁盘用量：后端已换算为 GB（`bytes / 1e9`），按 GB 显示，不做二次换算。 */
function gigabytes(value: number): string {
	if (!Number.isFinite(value)) return "—";
	return `${value.toFixed(1)} GB`;
}

export function AdminDashboardPage(): ReactNode {
	useDocumentTitle("控制台总览 · FloatCTF");
	const client = useClient();
	const queryClient = useAppQueryClient();
	const now = useNow(30_000);

	const summary = useQuery({
		queryKey: qk.admin.dashboard(),
		queryFn: () => call<DashboardSummary>(client.admin.dashboard.summary(), "总览聚合数据"),
		refetchInterval: 60_000,
		refetchIntervalInBackground: false,
	});

	const monitor = useQuery({
		queryKey: qk.admin.system(),
		queryFn: () => call<SystemInformation>(client.admin.system.monitor(), "系统监控"),
		refetchInterval: 60_000,
		refetchIntervalInBackground: false,
	});

	const version = useQuery({
		queryKey: qk.admin.version(),
		queryFn: () => call<string>(client.admin.system.version(), "平台版本"),
	});

	function refreshAll(): void {
		void queryClient.invalidateQueries({ queryKey: qk.admin.dashboard() });
		void queryClient.invalidateQueries({ queryKey: qk.admin.system() });
		void queryClient.invalidateQueries({ queryKey: qk.admin.version() });
	}

	return (
		<PageBody>
			<PageHeader
				title="控制台总览"
				description="平台规模、需要关注的事项、赛事进度与宿主资源。聚合数据与系统监控每 60 秒自动刷新。"
				actions={
					<Button variant="outline" onClick={refreshAll} disabled={summary.isFetching}>
						<RefreshCw className={summary.isFetching ? "animate-spin" : undefined} />
						刷新
					</Button>
				}
			/>

			<QueryState
				query={summary}
				skeleton={<LoadingBlock label="加载总览数据…" />}
				errorTitle="加载总览失败"
			>
				{(data) => (
					<>
						<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
							<StatCard
								label="用户"
								value={formatInt(data.stats.users)}
								hint="平台账号总数"
								icon={<Users className="size-5" />}
							/>
							<StatCard
								label="赛事"
								value={formatInt(data.stats.events)}
								hint="全部赛制合计"
								icon={<CalendarDays className="size-5" />}
							/>
							<StatCard
								label="题目"
								value={formatInt(data.stats.challenges)}
								hint="题库条目"
								icon={<Puzzle className="size-5" />}
							/>
							<StatCard
								label="GameBox"
								value={formatInt(data.stats.gameboxes)}
								hint="AWD 靶机库"
								icon={<Boxes className="size-5" />}
							/>
							<StatCard
								label="武器库"
								value={formatInt(data.stats.weapons)}
								icon={<Swords className="size-5" />}
							/>
							<StatCard
								label="实例"
								value={formatInt(data.stats.instances)}
								hint="题目环境实例"
								icon={<Container className="size-5" />}
							/>
							<StatCard
								label="公告"
								value={formatInt(data.stats.announcements)}
								icon={<Megaphone className="size-5" />}
							/>
							<StatCard
								label="讨论"
								value={formatInt(data.stats.discussions)}
								icon={<MessagesSquare className="size-5" />}
							/>
						</div>

						<AttentionSection summary={data} monitor={monitor.data} />

						<SectionCard
							title="赛事"
							description="赛事生命周期与 AWD 运行态。"
							actions={
								<Button variant="ghost" size="sm" asChild>
									<Link to="/admin/events">赛事管理</Link>
								</Button>
							}
							contentClassName="p-0"
						>
							{data.events.length === 0 ? (
								<div className="p-4">
									<EmptyBlock title="还没有赛事" description="创建赛事后会出现在这里。" />
								</div>
							) : (
								<DataTable
									data={data.events}
									getRowId={(row) => row.event_id}
									columns={eventColumns(now.getTime())}
									mobileCard={(row) => <EventMobileCard event={row} />}
									empty={<span>暂无赛事</span>}
								/>
							)}
						</SectionCard>

						<div className="grid gap-5 lg:grid-cols-2">
							<SectionCard title="最近解题" description="全站解题流水。">
								<ActivityList
									empty="暂无解题记录"
									items={data.activity.recent_solves.map((solve) => ({
										key: `${solve.nickname}-${solve.challenge_name}-${solve.solved_at}`,
										nickname: solve.nickname,
										avatar: solve.avatar,
										primary: `解出 ${solve.challenge_name}`,
										at: solve.solved_at,
									}))}
									now={now}
								/>
							</SectionCard>
							<SectionCard title="最近注册" description="新加入平台的账号。">
								<ActivityList
									empty="暂无注册记录"
									items={data.activity.recent_signups.map((signup) => ({
										key: `${signup.username}-${signup.created_at}`,
										nickname: signup.nickname || signup.username,
										avatar: signup.avatar,
										primary: signup.username,
										at: signup.created_at,
									}))}
									now={now}
								/>
							</SectionCard>
						</div>
					</>
				)}
			</QueryState>

			<SystemMonitorSection monitor={monitor} />

			<SectionCard title="平台版本" description="后端二进制版本（GET /api/admin/system/version）。">
				<QueryState query={version} skeleton={<LoadingBlock label="读取版本…" />}>
					{(value) =>
						value ? (
							<div className="flex items-center gap-2">
								<MonoText className="text-sm">{value}</MonoText>
								<CopyText value={value} label="版本号" />
							</div>
						) : (
							<EmptyBlock title="后端未返回版本号" />
						)
					}
				</QueryState>
			</SectionCard>
		</PageBody>
	);
}

// ── 需要关注 ────────────────────────────────────────────────────────────────

function AttentionSection({
	summary,
	monitor,
}: {
	summary: DashboardSummary;
	monitor: SystemInformation | undefined;
}) {
	const items: Array<{ key: string; tone: "danger" | "warning"; node: ReactNode }> = [];

	for (const alert of summary.attention.awd_alerts) {
		items.push({
			key: `awd-${alert.event_id}`,
			tone: "danger",
			node: (
				<span>
					AWD 赛事「{alert.title}」异常：状态 <b>{alert.status}</b>，阶段 <b>{alert.phase}</b>
					<span className="ml-1 text-muted-foreground">（{alert.event_id}）</span>
				</span>
			),
		});
	}

	for (const task of summary.attention.failed_tasks) {
		items.push({
			key: `task-${task.task_key}`,
			tone: "danger",
			node: (
				<span>
					计划任务「{task.task_name}」失败（第 {task.attempt_count}/{task.max_attempts} 次）：
					<span className="text-muted-foreground">{task.error_msg ?? "后端未返回错误详情"}</span>
					<span className="ml-1 text-muted-foreground">
						最近更新 {formatRelative(task.updated_at)}
					</span>
				</span>
			),
		});
	}

	if (summary.attention.error_logs_24h > 0) {
		items.push({
			key: "error-logs",
			tone: "warning",
			node: (
				<span>近 24 小时有 {formatInt(summary.attention.error_logs_24h)} 条 ERROR 日志</span>
			),
		});
	}

	for (const disk of monitor?.disks_info ?? []) {
		if (disk.usage_percent >= 90) {
			items.push({
				key: `disk-${disk.name}-${disk.mount_point}`,
				tone: "warning",
				node: (
					<span>
						磁盘 {disk.mount_point}（{disk.name}）使用率 {disk.usage_percent.toFixed(1)}%
					</span>
				),
			});
		}
	}

	return (
		<SectionCard title="需要关注" description="异常赛事、失败任务、错误日志与磁盘告警。">
			{items.length === 0 ? (
				<div className="flex flex-wrap items-center gap-2 text-sm">
					<TonePill tone="success">一切正常</TonePill>
					<span className="text-muted-foreground">
						无异常赛事、失败任务或资源告警
						{monitor ? "" : "（系统监控未加载，未包含磁盘告警）"}
					</span>
				</div>
			) : (
				<ul className="space-y-2">
					{items.map((item) => (
						<li
							key={item.key}
							className={
								item.tone === "danger"
									? "flex items-start gap-2 text-sm text-destructive"
									: "flex items-start gap-2 text-sm text-[var(--warning)]"
							}
						>
							<AlertTriangle className="mt-0.5 size-4 shrink-0" />
							<div className="min-w-0">{item.node}</div>
						</li>
					))}
				</ul>
			)}
		</SectionCard>
	);
}

// ── 赛事表 ──────────────────────────────────────────────────────────────────

function eventColumns(nowMs: number): DataTableColumn<SummaryEvent>[] {
	return [
		{
			id: "title",
			header: "赛事",
			cell: (row) => (
				<div className="min-w-0">
					<p className="truncate text-sm font-medium">{row.title}</p>
					<MonoText className="text-muted-foreground">{row.event_id}</MonoText>
				</div>
			),
			sortValue: (row) => row.title,
		},
		{
			id: "family",
			header: "赛制",
			cell: (row) => (
				<span className="text-sm">{EVENT_FAMILY_LABEL[row.family] ?? row.family}</span>
			),
			sortValue: (row) => row.family,
			hideBelow: "sm",
		},
		{
			id: "purpose",
			header: "用途 / 模式",
			cell: (row) => (
				<span className="text-xs text-muted-foreground">
					{EVENT_PURPOSE_LABEL[row.purpose] ?? row.purpose} ·{" "}
					{PARTICIPANT_MODE_LABEL[row.participant_mode] ?? row.participant_mode}
				</span>
			),
			hideBelow: "lg",
		},
		{
			id: "window",
			header: "时间窗",
			cell: (row) => (
				<span className="text-xs text-muted-foreground">
					{formatRange(row.start_time, row.end_time)}
				</span>
			),
			hideBelow: "md",
		},
		{
			id: "status",
			header: "状态",
			cell: (row) => {
				const status = computeEventStatus(row.start_time, row.end_time, nowMs);
				return (
					<div className="flex flex-wrap items-center gap-1">
						<TonePill tone={EVENT_STATUS_TONE[status]}>{EVENT_STATUS_LABEL[status]}</TonePill>
						<BooleanPill
							value={row.hidden}
							trueText="已隐藏"
							falseText="公开"
							trueTone="muted"
							falseTone="success"
						/>
					</div>
				);
			},
		},
		{
			id: "awd",
			header: "AWD",
			cell: (row) =>
				row.awd ? (
					<div className="flex flex-wrap items-center gap-1">
						<TonePill tone="info">{row.awd.status}</TonePill>
						<span className="text-xs text-muted-foreground">{row.awd.phase}</span>
					</div>
				) : (
					<span className="text-xs text-muted-foreground">—</span>
				),
			hideBelow: "sm",
		},
	];
}

function EventMobileCard({ event }: { event: SummaryEvent }) {
	const status = computeEventStatus(event.start_time, event.end_time);
	return (
		<div className="space-y-1.5">
			<div className="flex items-start justify-between gap-2">
				<p className="text-sm font-medium">{event.title}</p>
				<TonePill tone={EVENT_STATUS_TONE[status]}>{EVENT_STATUS_LABEL[status]}</TonePill>
			</div>
			<p className="text-xs text-muted-foreground">
				{EVENT_FAMILY_LABEL[event.family] ?? event.family} ·{" "}
				{EVENT_PURPOSE_LABEL[event.purpose] ?? event.purpose} ·{" "}
				{PARTICIPANT_MODE_LABEL[event.participant_mode] ?? event.participant_mode}
			</p>
			<p className="text-xs text-muted-foreground">
				{formatRange(event.start_time, event.end_time)}
			</p>
			<MonoText className="text-muted-foreground">{event.event_id}</MonoText>
		</div>
	);
}

// ── 动态列表 ────────────────────────────────────────────────────────────────

function ActivityList({
	items,
	empty,
	now,
}: {
	items: Array<{
		key: string;
		nickname: string;
		avatar: string | null;
		primary: string;
		at: string;
	}>;
	empty: string;
	now: Date;
}) {
	if (items.length === 0) return <EmptyBlock title={empty} />;
	return (
		<ul className="divide-y">
			{items.map((item) => (
				<li key={item.key} className="flex items-center gap-3 py-2.5">
					<UserAvatar name={item.nickname} avatar={item.avatar} size="sm" />
					<div className="min-w-0 flex-1">
						<p className="truncate text-sm">
							<span className="font-medium">{item.nickname}</span>
							<span className="text-muted-foreground"> {item.primary}</span>
						</p>
						<p className="text-xs text-muted-foreground">{formatRelative(item.at, now)}</p>
					</div>
				</li>
			))}
		</ul>
	);
}

// ── 系统监控 ────────────────────────────────────────────────────────────────

function SystemMonitorSection({ monitor }: { monitor: MonitorQuery }) {
	return (
		<SectionCard
			title="系统监控"
			description="宿主 CPU / 内存 / 磁盘 / 网卡 / Docker 概况，每 60 秒刷新。"
		>
			<QueryState
				query={monitor}
				skeleton={<LoadingBlock label="读取系统信息…" />}
				errorTitle="加载系统监控失败"
			>
				{(data) => (
					<div className="space-y-5">
						<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
							<StatCard
								label="CPU"
								value={`${formatInt(data.nb_cpu)} 核`}
								hint={
									data.name
										? `${data.name}${data.os_version ? ` ${data.os_version}` : ""}`
										: undefined
								}
								icon={<Cpu className="size-5" />}
							/>
							<StatCard
								label="运行时长"
								value={formatDuration(data.uptime)}
								hint={data.host_name ? `主机名 ${data.host_name}` : undefined}
								icon={<ServerCog className="size-5" />}
							/>
							<StatCard
								label="最高温度"
								value={temperatureText(data.max_temp)}
								hint={`平均 ${temperatureText(data.avg_temp)}`}
								icon={<Thermometer className="size-5" />}
								tone={
									data.max_temp >= 80 ? "danger" : data.max_temp >= 65 ? "warning" : "default"
								}
							/>
							<StatCard
								label="运行中容器"
								value={formatInt(data.docker_info.running_container_count)}
								hint={`镜像 ${formatInt(data.docker_info.image_count)} 个 · 占用 ${formatBytes(
									data.docker_info.total_disk,
								)}`}
								icon={<Container className="size-5" />}
							/>
						</div>

						<div className="grid gap-4 lg:grid-cols-2">
							<UsageMeter
								label="内存"
								used={data.used_memory}
								total={data.total_memory}
								format={formatBytes}
							/>
							<UsageMeter
								label="交换分区"
								used={data.used_swap}
								total={data.total_swap}
								format={formatBytes}
							/>
						</div>

						{data.kernel_version ? (
							<p className="text-xs text-muted-foreground">
								内核 <MonoText>{data.kernel_version}</MonoText>
							</p>
						) : null}

						<DiskSection disks={data.disks_info} />
						<NetworkSection interfaces={data.network_interfaces} />
						<DockerImagesSection monitor={data} />
					</div>
				)}
			</QueryState>
		</SectionCard>
	);
}

function UsageMeter({
	label,
	used,
	total,
	format,
}: {
	label: string;
	used: number;
	total: number;
	format: (value: number) => string;
}) {
	const value = percent(used, total);
	return (
		<div className="space-y-1.5 rounded-lg border p-3">
			<div className="flex flex-wrap items-center justify-between gap-2 text-sm">
				<span className="flex items-center gap-2 font-medium">
					{label}
					{value >= 90 ? <TonePill tone="danger">接近耗尽</TonePill> : null}
				</span>
				<span className="tnum font-mono text-xs text-muted-foreground">
					{format(used)} / {format(total)}（{value.toFixed(1)}%）
				</span>
			</div>
			<Progress value={value} aria-label={`${label}使用率`} />
		</div>
	);
}

function DiskSection({ disks }: { disks: DiskInformation[] }) {
	return (
		<div className="space-y-2">
			<h3 className="flex items-center gap-1.5 text-sm font-medium">
				<HardDrive className="size-4" /> 磁盘
			</h3>
			{disks.length === 0 ? (
				<EmptyBlock title="后端未返回磁盘信息" />
			) : (
				<div className="space-y-2">
					{disks.map((disk) => (
						<div
							key={`${disk.name}-${disk.mount_point}`}
							className="space-y-1.5 rounded-lg border p-3"
						>
							<div className="flex flex-wrap items-center justify-between gap-2 text-sm">
								<span className="flex flex-wrap items-center gap-2 font-medium">
									{disk.mount_point}
									<span className="text-xs text-muted-foreground">
										{disk.name} · {disk.file_system}
									</span>
									{disk.usage_percent >= 90 ? <TonePill tone="danger">空间告急</TonePill> : null}
								</span>
								<span className="tnum font-mono text-xs text-muted-foreground">
									{gigabytes(disk.used_space)} / {gigabytes(disk.total_space)}（
									{disk.usage_percent.toFixed(1)}%），可用 {gigabytes(disk.available_space)}
								</span>
							</div>
							<Progress
								value={Math.min(100, Math.max(0, disk.usage_percent))}
								aria-label={`${disk.mount_point} 使用率`}
							/>
						</div>
					))}
				</div>
			)}
		</div>
	);
}

function NetworkSection({ interfaces }: { interfaces: NetworkInterfaceInfo[] }) {
	return (
		<div className="space-y-2">
			<h3 className="text-sm font-medium">网卡</h3>
			{interfaces.length === 0 ? (
				<EmptyBlock title="后端未返回网卡信息" />
			) : (
				<div className="space-y-2">
					{interfaces.map((iface) => (
						<div
							key={iface.name}
							className="flex flex-wrap items-center justify-between gap-2 rounded-lg border p-3 text-sm"
						>
							<div className="min-w-0">
								<p className="font-medium">{iface.name}</p>
								<p className="flex flex-wrap gap-1 text-xs text-muted-foreground">
									{iface.ip_addresses.length === 0 ? (
										<span>无地址</span>
									) : (
										iface.ip_addresses.map((address) => (
											<MonoText key={address}>{address}</MonoText>
										))
									)}
								</p>
							</div>
							<div className="tnum font-mono text-xs text-muted-foreground">
								<div>
									收 {formatBytes(iface.received)} · 发 {formatBytes(iface.transmitted)}
								</div>
								<div>
									实时 ↓ {formatBytes(iface.recv_rate)}/s · ↑ {formatBytes(iface.transmit_rate)}/s
								</div>
							</div>
						</div>
					))}
				</div>
			)}
		</div>
	);
}

function DockerImagesSection({ monitor }: { monitor: SystemInformation }) {
	return (
		<div className="space-y-2">
			<h3 className="text-sm font-medium">
				Docker 镜像（{formatInt(monitor.docker_info.image_count)}）
			</h3>
			{monitor.docker_info.images.length === 0 ? (
				<EmptyBlock title="没有本地镜像" />
			) : (
				<ul className="max-h-72 space-y-1.5 overflow-y-auto pr-1">
					{monitor.docker_info.images.map((image) => (
						<li
							key={image.id}
							className="flex flex-wrap items-center justify-between gap-2 rounded-md border px-3 py-1.5 text-xs"
						>
							<MonoText className="min-w-0">{image.repo_tags.join(", ") || image.id}</MonoText>
							<span className="tnum font-mono text-muted-foreground">
								{formatBytes(image.size)}
							</span>
						</li>
					))}
				</ul>
			)}
		</div>
	);
}
