import {
	AlertIcon,
	CalendarIcon,
	CheckCircleIcon,
	ContainerIcon,
	DatabaseIcon,
	LogIcon,
	PackageIcon,
	PeopleIcon,
	ServerIcon,
	TrophyIcon,
	XCircleIcon,
} from "@primer/octicons-react";
import { Avatar, ProgressBar, Spinner } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

import { adminApi } from "@/api";
import type { DashboardSummary } from "@/api/admin/dashboard";
import { systemInformationQueryOptions } from "@/api/queries";
import {
	type EventStatus,
	EventStatusBadge,
	computeEventStatus,
} from "@/components";
import { AppLink } from "@/navigation";
import { AdminRouteGuard } from "@/routes/admin/route";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/admin/dashboard")({
	component: RouteComponent,
	loader: async ({ context }) => {
		await AdminRouteGuard();
		await context.queryClient.ensureQueryData(systemInformationQueryOptions());
	},
});
export type SystemInformation = {
	name?: string;
	kernel_version?: string;
	os_version?: string;
	host_name?: string;
	uptime: number;
	total_memory: number;
	used_memory: number;
	total_swap: number;
	used_swap: number;
	avg_temp: number;
	max_temp: number;
	nb_cpu: number;
	disks_info: DiskInformation[];
	network_interfaces: NetworkInterfaceInfo[];
	docker_info: DockerInformation;
};

export type DiskInformation = {
	name: string;
	mount_point: string;
	file_system: string;
	total_space: number;
	available_space: number;
	used_space: number;
	usage_percent: number;
};

export type NetworkInterfaceInfo = {
	name: string;
	ip_addresses: string[];
	received: number;
	transmitted: number;
	recv_rate: number;
	transmit_rate: number;
};

export type DockerImageInfo = {
	id: string;
	repo_tags: string[];
	size: number;
};

export type DockerInformation = {
	image_count: number;
	images: DockerImageInfo[];
	running_container_count: number;
	total_disk: number;
};

// ─────────────────────────────────────────────────────────────────────────────
// Primer 风格基础件（颜色走 @primer/css light 变量，暗色主题可复用）
// ─────────────────────────────────────────────────────────────────────────────

const CARD =
	"border border-[var(--borderColor-default)] rounded-md bg-[var(--bgColor-default)]";

type Tone = "neutral" | "blue" | "green" | "amber" | "red";

const CHIP_TONES: Record<Tone, string> = {
	neutral: "bg-[var(--bgColor-muted)] text-[var(--fgColor-muted)]",
	blue: "bg-[var(--bgColor-accent)] text-[var(--fgColor-accent)]",
	green: "bg-[var(--bgColor-success)] text-[var(--fgColor-success)]",
	amber: "bg-[var(--bgColor-attention)] text-[var(--fgColor-attention)]",
	red: "bg-[var(--bgColor-danger)] text-[var(--fgColor-danger)]",
};

function Chip({
	tone = "neutral",
	children,
}: {
	tone?: Tone;
	children: React.ReactNode;
}) {
	return (
		<span
			className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium whitespace-nowrap ${CHIP_TONES[tone]}`}
		>
			{children}
		</span>
	);
}

const AWD_STATUS_TONES: Record<string, Tone> = {
	running: "green",
	paused: "amber",
	verified: "blue",
	deploying: "blue",
	deployed: "blue",
	prechecking: "blue",
	draft: "neutral",
	configuring: "neutral",
	deploy_failed: "red",
	network_error: "red",
	verification_failed: "red",
	start_blocked: "red",
	finished: "neutral",
	archived: "neutral",
};

const AWD_PHASE_TONES: Record<string, Tone> = {
	attack: "red",
	hardening: "blue",
	pause: "amber",
};

type EventState = EventStatus;

function eventState(event: {
	start_time: string;
	end_time?: string | null;
}): EventState {
	return computeEventStatus(event.start_time, event.end_time);
}

function timeDelta(targetMs: number): string {
	const diff = targetMs - Date.now();
	const abs = Math.abs(diff);
	if (abs < 60_000) return diff >= 0 ? "即将开始" : "刚刚";
	const mins = Math.floor(abs / 60_000);
	if (mins < 60) return diff >= 0 ? `${mins} 分钟后` : `${mins} 分钟前`;
	const hours = Math.floor(mins / 60);
	if (hours < 48) return diff >= 0 ? `${hours} 小时后` : `${hours} 小时前`;
	return diff >= 0
		? `${Math.floor(hours / 24)} 天后`
		: `${Math.floor(hours / 24)} 天前`;
}

function AvatarOrInitial({
	nickname,
	avatar,
	size = 20,
}: {
	nickname: string;
	avatar?: string | null;
	size?: number;
}) {
	if (avatar) {
		return <Avatar src={avatar} size={size} />;
	}
	return (
		<div
			className="flex items-center justify-center rounded-full bg-[var(--bgColor-muted)] text-[var(--fgColor-muted)] font-medium flex-shrink-0"
			style={{ width: size, height: size, fontSize: Math.round(size * 0.5) }}
		>
			{(nickname || "?").slice(0, 1).toUpperCase()}
		</div>
	);
}

function Empty() {
	return (
		<div className="text-xs text-[var(--fgColor-muted)] px-3 py-3">
			暂无数据
		</div>
	);
}

// ─────────────────────────────────────────────────────────────────────────────
// 需要处理的事项
// ─────────────────────────────────────────────────────────────────────────────

function AttentionPanel({
	summary,
	disks,
	stoppedContainers,
}: {
	summary: DashboardSummary;
	disks: DiskInformation[];
	stoppedContainers: number;
}) {
	const items: Array<{ key: string; tone: Tone; node: React.ReactNode }> = [];

	for (const alert of summary.attention.awd_alerts) {
		items.push({
			key: `awd-${alert.event_id}`,
			tone: "red",
			node: (
				<>
					<AlertIcon />
					<span>
						AWD 赛事
						<AppLink
							to={`/admin/events/awd/${alert.event_id}`}
							className="font-medium underline decoration-dotted underline-offset-2"
						>
							「{alert.title}」
						</AppLink>
						处于异常状态：<span className="font-medium">{alert.status}</span>
					</span>
				</>
			),
		});
	}
	for (const task of summary.attention.failed_tasks) {
		items.push({
			key: `task-${task.task_key}`,
			tone: "red",
			node: (
				<>
					<XCircleIcon />
					<span>
						定时任务失败：
						<AppLink
							to="/admin/scheduled_tasks"
							className="font-medium underline decoration-dotted underline-offset-2"
						>
							{task.task_name}
						</AppLink>
						<span className="opacity-70">
							（{task.error_msg ?? task.task_key}，第 {task.attempt_count}/
							{task.max_attempts} 次）
						</span>
					</span>
				</>
			),
		});
	}
	if (summary.attention.error_logs_24h > 0) {
		items.push({
			key: "error-logs",
			tone: "amber",
			node: (
				<>
					<LogIcon />
					<span>
						<AppLink
							to="/admin/logs"
							className="font-medium underline decoration-dotted underline-offset-2"
						>
							{summary.attention.error_logs_24h} 条 ERROR 日志
						</AppLink>
						（近 24 小时）
					</span>
				</>
			),
		});
	}
	for (const disk of disks.filter((d) => d.usage_percent >= 90)) {
		items.push({
			key: "error-logs",
			tone: "amber",
			node: (
				<>
					<DatabaseIcon />
					<span>
						磁盘 {disk.mount_point} 使用率{" "}
						<span className="font-medium">
							{Math.round(disk.usage_percent)}%
						</span>
					</span>
				</>
			),
		});
	}
	if (stoppedContainers > 0) {
		items.push({
			key: "error-logs",
			tone: "amber",
			node: (
				<>
					<ContainerIcon />
					<span>
						<AppLink
							to="/admin/docker"
							className="font-medium underline decoration-dotted underline-offset-2"
						>
							{stoppedContainers} 个容器未运行
						</AppLink>
					</span>
				</>
			),
		});
	}

	if (items.length === 0) {
		return (
			<div className={`${CARD} p-3 flex items-center gap-2`}>
				<CheckCircleIcon className="text-[var(--fgColor-success)]" />
				<span className="font-medium text-sm">一切正常</span>
				<span className="text-xs text-[var(--fgColor-muted)]">
					无异常赛事、失败任务或资源告警
				</span>
			</div>
		);
	}
	return (
		<div className={`${CARD} p-1.5`}>
			{items.map((item, index) => (
				<div
					key={item.key}
					className={`flex items-center gap-2 px-2.5 py-1.5 rounded-md text-sm ${
						item.tone === "red"
							? "text-[var(--fgColor-danger)]"
							: "text-[var(--fgColor-attention)]"
					}`}
				>
					{item.node}
				</div>
			))}
		</div>
	);
}

// ─────────────────────────────────────────────────────────────────────────────
// 平台规模统计块（Primer repo 风格）
// ─────────────────────────────────────────────────────────────────────────────

function StatBlocks({
	summary,
	runningContainers,
}: {
	summary: DashboardSummary;
	runningContainers: number;
}) {
	const blocks = [
		{
			label: "Users",
			value: summary.stats.users,
			href: "/admin/users",
			icon: <PeopleIcon />,
		},
		{
			label: "Events",
			value: summary.stats.events,
			href: "/admin/events",
			icon: <CalendarIcon />,
		},
		{
			label: "Challenges",
			value: summary.stats.challenges,
			href: "/admin/challenges",
			icon: <TrophyIcon />,
		},
		{
			label: "Instances",
			value: summary.stats.instances,
			href: "/admin/instances",
			icon: <ServerIcon />,
		},
		{
			label: "AWD GameBoxes",
			value: summary.stats.gameboxes,
			href: "/admin/awd/gameboxes",
			icon: <PackageIcon />,
		},
		{
			label: "Docker Running",
			value: runningContainers,
			href: "/admin/docker",
			icon: <ContainerIcon />,
		},
	];
	return (
		<div
			className={`${CARD} flex flex-wrap divide-x divide-[var(--borderColor-default)]`}
		>
			{blocks.map((block) => (
				<AppLink
					key={block.label}
					to={block.href}
					className="flex-1 min-w-[150px] px-4 py-3 hover:bg-[var(--bgColor-muted)] flex items-center gap-2.5"
				>
					<span className="text-[var(--fgColor-muted)]">{block.icon}</span>
					<span className="text-lg font-semibold tabular-nums">
						{block.value}
					</span>
					<span className="text-xs text-[var(--fgColor-muted)]">
						{block.label}
					</span>
				</AppLink>
			))}
		</div>
	);
}

// ─────────────────────────────────────────────────────────────────────────────
// 赛事列表（进行中 > 即将开始 > 已结束）
// ─────────────────────────────────────────────────────────────────────────────

function EventRow({ event }: { event: DashboardSummary["events"][number] }) {
	const state = eventState(event);
	const isAwd = event.family === "awd";
	const href = isAwd
		? `/admin/events/awd/${event.event_id}`
		: `/admin/events/jeopardy/${event.event_id}`;

	const dotColor =
		state === "ongoing"
			? "bg-[var(--fgColor-success)]"
			: state === "upcoming"
				? "bg-[var(--fgColor-accent)]"
				: "bg-[var(--fgColor-muted)]";

	let rightText: string;
	if (state === "ongoing") {
		rightText = event.end_time
			? `剩 ${timeDelta(new Date(event.end_time).getTime())}`
			: "进行中";
	} else if (state === "upcoming") {
		rightText = timeDelta(new Date(event.start_time).getTime());
	} else {
		rightText = event.end_time
			? `结束 ${timeDelta(new Date(event.end_time).getTime())}`
			: "已结束";
	}

	return (
		<AppLink
			to={href}
			className="flex items-center gap-3 px-3 py-2 rounded-md hover:bg-[var(--bgColor-muted)]"
		>
			<span className={`w-2 h-2 rounded-full flex-shrink-0 ${dotColor}`} />
			<div className="flex-1 min-w-0">
				<div className="flex items-center gap-2 min-w-0">
					<span className="font-medium truncate">{event.title}</span>
					{event.hidden && (
						<span className="text-xs text-[var(--fgColor-muted)] flex-shrink-0">
							hidden
						</span>
					)}
					{isAwd && <Chip tone="blue">AWD</Chip>}
				</div>
				<div className="text-xs text-[var(--fgColor-muted)]">
					{DatetimeToShow(event.start_time)} → {DatetimeToShow(event.end_time)}
				</div>
			</div>
			{isAwd && event.awd ? (
				<div className="flex items-center gap-1.5 flex-shrink-0">
					<Chip tone={AWD_STATUS_TONES[event.awd.status] ?? "neutral"}>
						{event.awd.status}
					</Chip>
					{(event.awd.status === "running" ||
						event.awd.status === "paused") && (
						<Chip tone={AWD_PHASE_TONES[event.awd.phase] ?? "neutral"}>
							{event.awd.phase}
						</Chip>
					)}
				</div>
			) : (
				<EventStatusBadge
					startTime={event.start_time}
					endTime={event.end_time}
				/>
			)}
			<span className="text-xs text-[var(--fgColor-muted)] w-24 text-right flex-shrink-0">
				{rightText}
			</span>
		</AppLink>
	);
}

const EVENT_STATE_ORDER: Record<EventState, number> = {
	ongoing: 0,
	upcoming: 1,
	ended: 2,
	unknown: 3,
};

function Competitions({
	events,
}: {
	events: DashboardSummary["events"];
}) {
	const sorted = [...events].sort((a, b) => {
		const stateDiff =
			EVENT_STATE_ORDER[eventState(a)] - EVENT_STATE_ORDER[eventState(b)];
		if (stateDiff !== 0) return stateDiff;
		return new Date(a.start_time).getTime() - new Date(b.start_time).getTime();
	});
	return (
		<div className={CARD}>
			<div className="px-3 py-2 border-b border-[var(--borderColor-default)] flex items-center justify-between">
				<h2 className="text-sm font-semibold">Competitions</h2>
				<AppLink
					to="/admin/events"
					className="text-xs text-[var(--accent-fg)] hover:underline"
				>
					All events →
				</AppLink>
			</div>
			<div className="p-1.5">
				{sorted.length === 0 ? (
					<Empty />
				) : (
					sorted.map((event) => <EventRow key={event.event_id} event={event} />)
				)}
			</div>
		</div>
	);
}

// ─────────────────────────────────────────────────────────────────────────────
// 近期活动
// ─────────────────────────────────────────────────────────────────────────────

function Activity({
	summary,
}: {
	summary: DashboardSummary;
}) {
	return (
		<div className="grid grid-cols-1 lg:grid-cols-2 gap-3">
			<div className={CARD}>
				<div className="px-3 py-2 border-b border-[var(--borderColor-default)]">
					<h2 className="text-sm font-semibold">Recent Solves</h2>
				</div>
				{summary.activity.recent_solves.length === 0 ? (
					<Empty />
				) : (
					summary.activity.recent_solves.map((solve) => (
						<div
							key={`${solve.nickname}-${solve.challenge_name}-${solve.solved_at}`}
							className="flex items-center gap-2 px-3 py-1.5 text-sm"
						>
							<AvatarOrInitial
								nickname={solve.nickname}
								avatar={solve.avatar}
							/>
							<span className="truncate min-w-0">
								<span className="font-medium">{solve.nickname}</span> 解出{" "}
								<span className="text-[var(--accent-fg)]">
									{solve.challenge_name}
								</span>
							</span>
							<span className="ml-auto text-xs text-[var(--fgColor-muted)] whitespace-nowrap">
								{timeDelta(new Date(solve.solved_at).getTime())}
							</span>
						</div>
					))
				)}
			</div>
			<div className={CARD}>
				<div className="px-3 py-2 border-b border-[var(--borderColor-default)] flex items-center justify-between">
					<h2 className="text-sm font-semibold">Recent Signups</h2>
					<AppLink
						to="/admin/users"
						className="text-xs text-[var(--accent-fg)] hover:underline"
					>
						All users →
					</AppLink>
				</div>
				{summary.activity.recent_signups.length === 0 ? (
					<Empty />
				) : (
					summary.activity.recent_signups.map((user) => (
						<div
							key={`${user.username}-${user.created_at}`}
							className="flex items-center gap-2 px-3 py-1.5 text-sm"
						>
							<AvatarOrInitial nickname={user.nickname} avatar={user.avatar} />
							<span className="truncate min-w-0">
								<span className="font-medium">{user.nickname}</span>{" "}
								<span className="text-[var(--fgColor-muted)]">
									@{user.username}
								</span>
							</span>
							<span className="ml-auto text-xs text-[var(--fgColor-muted)] whitespace-nowrap">
								{timeDelta(new Date(user.created_at).getTime())}
							</span>
						</div>
					))
				)}
			</div>
		</div>
	);
}

// ─────────────────────────────────────────────────────────────────────────────
// 宿主与基础设施（压缩展示）
// ─────────────────────────────────────────────────────────────────────────────

function Infrastructure({ data }: { data: SystemInformation }) {
	const memPercent = Math.round((data.used_memory * 100) / data.total_memory);
	return (
		<div className={CARD}>
			<div className="px-3 py-2 border-b border-[var(--borderColor-default)]">
				<h2 className="text-sm font-semibold">Host &amp; Infrastructure</h2>
			</div>
			<div className="grid grid-cols-1 md:grid-cols-2 gap-4 p-3">
				<div className="space-y-3">
					<div>
						<div className="flex justify-between text-xs text-[var(--fgColor-muted)] mb-1">
							<span>Memory</span>
							<span>
								{(data.used_memory / 1024 ** 3).toFixed(1)} /{" "}
								{(data.total_memory / 1024 ** 3).toFixed(1)} GB
							</span>
						</div>
						<ProgressBar progress={memPercent} />
					</div>
					{data.disks_info.map((disk) => (
						<div key={disk.mount_point}>
							<div className="flex justify-between text-xs text-[var(--fgColor-muted)] mb-1">
								<span className="truncate mr-2">{disk.mount_point}</span>
								<span>{Math.round(disk.usage_percent)}%</span>
							</div>
							<ProgressBar progress={Math.round(disk.usage_percent)} />
						</div>
					))}
				</div>
				<div className="text-sm space-y-1 text-[var(--fgColor-muted)]">
					<div>
						Docker：{data.docker_info.running_container_count} 运行 /{" "}
						{data.docker_info.image_count} 镜像，共{" "}
						{(data.docker_info.total_disk / 1024 ** 3).toFixed(1)} GB
					</div>
					<div>
						OS：{data.name} {data.os_version}
					</div>
					<div>Kernel：{data.kernel_version}</div>
					<div>CPU：{data.nb_cpu} cores</div>
					<div>Uptime：{Math.floor(data.uptime / 3600)} h</div>
				</div>
			</div>
		</div>
	);
}

// ─────────────────────────────────────────────────────────────────────────────
// 页面
// ─────────────────────────────────────────────────────────────────────────────

function RouteComponent() {
	const {
		data: d,
		isLoading: monitorLoading,
		isError: monitorError,
	} = useQuery({
		...systemInformationQueryOptions(),
		refetchInterval: 1000 * 60,
	});
	const monitor = d?.data;

	const summaryQuery = useQuery({
		queryKey: ["admin-dashboard", "summary"],
		queryFn: async () => (await adminApi.dashboard.summary()).data ?? null,
		staleTime: 60_000,
		refetchInterval: 60_000,
	});
	const containersQuery = useQuery({
		queryKey: ["admin-dashboard", "docker-containers"],
		queryFn: async () =>
			(await adminApi.docker.fetchContainers({ limit: 500 })).data ?? [],
		staleTime: 60_000,
	});

	if (monitorLoading) {
		return <Spinner size="large" />;
	}
	if (monitorError || !monitor) {
		return <div>Error loading system info</div>;
	}

	const summary = summaryQuery.data;
	const stoppedContainers = containersQuery.data
		? containersQuery.data.filter((c) => c.status !== "Running").length
		: 0;

	return (
		<div className="grid gap-3 p-3">
			<div className="flex items-baseline justify-between">
				<h1 className="text-lg font-semibold">Dashboard</h1>
				<span className="text-xs text-[var(--fgColor-muted)]">
					{monitor.host_name} · 数据每分钟刷新
				</span>
			</div>

			{summary && (
				<>
					<StatBlocks
						summary={summary}
						runningContainers={monitor.docker_info.running_container_count}
					/>
					<Competitions events={summary.events} />
					<AttentionPanel
						summary={summary}
						disks={monitor.disks_info}
						stoppedContainers={stoppedContainers}
					/>
					<Activity summary={summary} />
				</>
			)}
			{!summary && summaryQuery.isLoading && <Spinner size="medium" />}

			<Infrastructure data={monitor} />
		</div>
	);
}
