import {
	CalendarIcon,
	ClockIcon,
	CommentDiscussionIcon,
	ContainerIcon,
	GiftIcon,
	MegaphoneIcon,
	PackageIcon,
	PeopleIcon,
	ServerIcon,
	TrophyIcon,
} from "@primer/octicons-react";
import { Avatar, ProgressBar, Spinner } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

import { adminApi } from "@/api";
import { type QueryParams, type UniResponse } from "@/api/axios";
import { systemInformationQueryOptions } from "@/api/queries";
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
// 平台统计卡片：每个列表接口用 limit=1 拿 meta.total 计数，整卡可点击跳转。
// ─────────────────────────────────────────────────────────────────────────────

type CountFetcher = (params: QueryParams) => Promise<UniResponse<unknown[]>>;

type StatCard = {
	key: string;
	label: string;
	href: string;
	icon: React.ReactNode;
	fetch: CountFetcher;
};

const STAT_CARDS: StatCard[] = [
	{
		key: "users",
		label: "Users",
		href: "/admin/users",
		icon: <PeopleIcon />,
		fetch: (params) => adminApi.users.fetch(params),
	},
	{
		key: "events",
		label: "Events",
		href: "/admin/events",
		icon: <CalendarIcon />,
		fetch: (params) => adminApi.events.fetch(params),
	},
	{
		key: "challenges",
		label: "Challenges",
		href: "/admin/challenges",
		icon: <TrophyIcon />,
		fetch: (params) => adminApi.challenges.fetch(params),
	},
	{
		key: "weapons",
		label: "Weapons",
		href: "/admin/weapons",
		icon: <GiftIcon />,
		fetch: (params) => adminApi.weapons.fetch(params),
	},
	{
		key: "announcements",
		label: "Announcements",
		href: "/admin/announcements",
		icon: <MegaphoneIcon />,
		fetch: (params) => adminApi.announcements.fetch(params),
	},
	{
		key: "discussions",
		label: "Discussions",
		href: "/admin/discussions",
		icon: <CommentDiscussionIcon />,
		fetch: (params) => adminApi.discussions.fetch(params),
	},
	{
		key: "instances",
		label: "Instances",
		href: "/admin/instances",
		icon: <ServerIcon />,
		fetch: (params) => adminApi.instances.fetch(params),
	},
	{
		key: "gameboxes",
		label: "AWD GameBoxes",
		href: "/admin/awd/gameboxes",
		icon: <PackageIcon />,
		fetch: (params) => adminApi.awd.listGameboxes(params),
	},
	{
		key: "scheduled-tasks",
		label: "Scheduled Tasks",
		href: "/admin/scheduled_tasks",
		icon: <ClockIcon />,
		fetch: (params) => adminApi.scheduled_tasks.fetch(params),
	},
];

function StatCardView({
	label,
	href,
	icon,
	value,
	loading,
}: {
	label: string;
	href: string;
	icon: React.ReactNode;
	value: number | undefined;
	loading: boolean;
}) {
	return (
		<AppLink
			to={href}
			className="border border-gray-300 rounded-lg p-3 flex flex-col gap-1 hover:border-gray-500 hover:shadow-sm transition-colors"
		>
			<div className="flex items-center gap-2 text-sm text-gray-600">
				<span className="text-gray-500 flex-shrink-0">{icon}</span>
				<span className="truncate">{label}</span>
			</div>
			<div className="text-2xl font-semibold tabular-nums">
				{loading ? "–" : value}
			</div>
		</AppLink>
	);
}

function CountCard({ card }: { card: StatCard }) {
	const { data, isLoading } = useQuery({
		queryKey: ["admin-dashboard", "count", card.key],
		queryFn: async () => {
			const res = await card.fetch({ limit: 1, page: 1 });
			return res.meta?.total ?? res.data?.length ?? 0;
		},
		staleTime: 60_000,
	});
	return (
		<StatCardView
			label={card.label}
			href={card.href}
			icon={card.icon}
			value={data}
			loading={isLoading}
		/>
	);
}

// ─────────────────────────────────────────────────────────────────────────────
// 最近动态面板
// ─────────────────────────────────────────────────────────────────────────────

const SKELETON_ROWS = ["row-1", "row-2", "row-3"];

function LoadingRows() {
	return (
		<div className="space-y-2 py-1">
			{SKELETON_ROWS.map((key) => (
				<div key={key} className="h-5 bg-gray-100 rounded animate-pulse" />
			))}
		</div>
	);
}

function EmptyRows() {
	return <div className="text-xs text-gray-400 py-2">暂无数据</div>;
}

function Panel({
	title,
	href,
	children,
}: {
	title: string;
	href: string;
	children: React.ReactNode;
}) {
	return (
		<div className="border border-gray-300 rounded-lg p-3">
			<div className="flex items-center justify-between mb-2">
				<h2 className="text-base font-semibold">{title}</h2>
				<AppLink to={href} className="text-xs text-blue-600 hover:underline">
					View all →
				</AppLink>
			</div>
			{children}
		</div>
	);
}

function RecentUsers() {
	const { data, isLoading } = useQuery({
		queryKey: ["admin-dashboard", "recent", "users"],
		queryFn: async () =>
			(await adminApi.users.fetch({ limit: 5, page: 1 })).data ?? [],
		staleTime: 60_000,
	});
	return (
		<Panel title="Recent Users" href="/admin/users">
			{isLoading ? (
				<LoadingRows />
			) : data === undefined || data.length === 0 ? (
				<EmptyRows />
			) : (
				data.map((user) => (
					<div
						key={user.id}
						className="flex items-center gap-2 justify-between py-1"
					>
						<div className="flex items-center gap-2 min-w-0">
							{user.avatar ? (
								<Avatar src={user.avatar} size={20} />
							) : (
								<div
									className="flex items-center justify-center rounded-full bg-gray-200 text-gray-500 font-medium flex-shrink-0"
									style={{ width: 20, height: 20, fontSize: 10 }}
								>
									{(user.nickname ?? user.username ?? "?")
										.slice(0, 1)
										.toUpperCase()}
								</div>
							)}
							<span className="truncate font-medium">
								{user.nickname || user.username}
							</span>
						</div>
						<time className="text-xs text-gray-500 flex-shrink-0">
							{DatetimeToShow(user.created_at)}
						</time>
					</div>
				))
			)}
		</Panel>
	);
}

function RecentEvents() {
	const { data, isLoading } = useQuery({
		queryKey: ["admin-dashboard", "recent", "events"],
		queryFn: async () =>
			(await adminApi.events.fetch({ limit: 5, page: 1 })).data ?? [],
		staleTime: 60_000,
	});
	return (
		<Panel title="Recent Events" href="/admin/events">
			{isLoading ? (
				<LoadingRows />
			) : data === undefined || data.length === 0 ? (
				<EmptyRows />
			) : (
				data.map((event) => (
					<div key={event.id} className="py-1">
						<div className="flex items-center justify-between gap-2">
							<span className="truncate font-medium">{event.title}</span>
							<span
								className={`text-xs px-1.5 py-0.5 rounded flex-shrink-0 ${
									event.type === "awd_team"
										? "bg-purple-100 text-purple-700"
										: "bg-blue-100 text-blue-700"
								}`}
							>
								{event.type === "awd_team" ? "AWD" : "Jeopardy"}
							</span>
						</div>
						<div className="text-xs text-gray-500">
							{DatetimeToShow(event.start_time)} →{" "}
							{DatetimeToShow(event.end_time)}
						</div>
					</div>
				))
			)}
		</Panel>
	);
}

function RecentChallenges() {
	const { data, isLoading } = useQuery({
		queryKey: ["admin-dashboard", "recent", "challenges"],
		queryFn: async () =>
			(await adminApi.challenges.fetch({ limit: 5, page: 1 })).data ?? [],
		staleTime: 60_000,
	});
	return (
		<Panel title="Recent Challenges" href="/admin/challenges">
			{isLoading ? (
				<LoadingRows />
			) : data === undefined || data.length === 0 ? (
				<EmptyRows />
			) : (
				data.map((challenge) => (
					<div
						key={challenge.id}
						className="flex items-center justify-between gap-2 py-1"
					>
						<div className="flex items-center gap-2 min-w-0">
							<span className="text-gray-500 flex-shrink-0">
								<TrophyIcon size={14} />
							</span>
							<span className="truncate font-medium">{challenge.name}</span>
						</div>
						<div className="flex items-center gap-2 flex-shrink-0">
							{challenge.category && (
								<span className="text-xs text-gray-500">
									{challenge.category}
								</span>
							)}
							<span className="text-xs text-gray-400">
								{DatetimeToShow(challenge.updated_at)}
							</span>
						</div>
					</div>
				))
			)}
		</Panel>
	);
}

function RouteComponent() {
	const {
		data: d,
		isLoading,
		isError,
	} = useQuery({
		...systemInformationQueryOptions(),
		refetchInterval: 1000 * 60,
	});
	const data = d?.data;
	if (isLoading) {
		return <Spinner size="large" />;
	}
	if (isError || !data) {
		return <div>Error loading system info</div>;
	}

	return (
		<div className="grid gap-3 p-3">
			<div>
				<h1 className="text-lg font-semibold">Dashboard</h1>
				<p className="text-xs text-gray-500 mt-0.5">
					Host {data.host_name} · {data.name} {data.os_version} · Uptime{" "}
					{Math.floor(data.uptime / 3600)}h · 每分钟刷新
				</p>
			</div>

			{/* 平台统计 */}
			<div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 gap-3">
				{STAT_CARDS.map((card) => (
					<CountCard key={card.key} card={card} />
				))}
				<StatCardView
					label="Docker Containers"
					href="/admin/docker"
					icon={<ContainerIcon />}
					value={data.docker_info.running_container_count}
					loading={false}
				/>
			</div>

			{/* 最近动态 */}
			<div className="grid grid-cols-1 lg:grid-cols-3 gap-3">
				<RecentUsers />
				<RecentEvents />
				<RecentChallenges />
			</div>

			{/* 宿主与基础设施 */}
			<h2 className="text-base font-semibold">Host &amp; Infrastructure</h2>
			<div className="grid grid-cols-1 md:grid-cols-2 gap-3">
				{/* System Info */}
				<div className="border border-gray-300 rounded-lg p-3">
					<h2 className="text-base font-semibold mb-1">System Overview</h2>
					<div>
						OS: {data.name} {data.os_version}
					</div>
					<div>Kernel: {data.kernel_version}</div>
					<div>Host: {data.host_name}</div>
					<div>Uptime: {Math.floor(data.uptime / 3600)} h</div>
					<div>CPU cores: {data.nb_cpu}</div>
				</div>

				{/* Resources (Memory + Disks stacked) */}
				<div className="space-y-2">
					{/* Memory */}
					<div className="border border-gray-300 rounded-lg p-3">
						<h2 className="text-base font-semibold mb-1">Memory</h2>
						<div className="mb-1">
							{(data.used_memory / 1024 ** 3).toFixed(1)} /{" "}
							{(data.total_memory / 1024 ** 3).toFixed(1)} GB
							<ProgressBar
								progress={Math.round(
									(data.used_memory * 100) / data.total_memory,
								)}
								className="mt-1"
							/>
						</div>
						<div>
							Swap: {(data.used_swap / 1024 ** 3).toFixed(1)} /{" "}
							{(data.total_swap / 1024 ** 3).toFixed(1)} GB
							<ProgressBar
								progress={
									data.total_swap
										? Math.round((data.used_swap * 100) / data.total_swap)
										: 0
								}
								className="mt-1"
							/>
						</div>
					</div>

					{/* Disks */}
					<div className="border border-gray-300 rounded-lg p-3">
						<h2 className="text-base font-semibold mb-1">Disks</h2>
						{data.disks_info.map((disk) => (
							<div key={disk.mount_point} className="mb-1">
								<span className="font-medium mr-2">{disk.mount_point}</span>
								{disk.used_space.toFixed(1)} / {disk.total_space.toFixed(1)} GB
								({Math.round(disk.usage_percent)}%)
								<ProgressBar
									progress={Math.round(disk.usage_percent)}
									className="mt-1"
								/>
							</div>
						))}
					</div>
				</div>
			</div>

			{/* Network */}
			{data.network_interfaces?.length > 0 && (
				<div className="border border-gray-300 rounded-lg p-3">
					<h2 className="text-base font-semibold mb-1">Network Interfaces</h2>
					{data.network_interfaces
						.filter((iface) => iface.ip_addresses.length > 0)
						.map((iface) => (
							<div key={iface.name} className="mb-1">
								<span className="font-medium mr-2">{iface.name}</span>
								{iface.ip_addresses.join(", ")}
								<br />
								Rx: {(iface.received / 1024 / 1024).toFixed(2)} MB, Tx:{" "}
								{(iface.transmitted / 1024 / 1024).toFixed(2)} MB
							</div>
						))}
				</div>
			)}

			{/* Docker */}
			<div className="border border-gray-300 rounded-lg p-3">
				<h2 className="text-base font-semibold mb-1">Docker</h2>
				<div>Images: {data.docker_info.image_count}</div>
				<div>
					Running containers: {data.docker_info.running_container_count}
				</div>
				<div>
					Disk used: {(data.docker_info.total_disk / 1024 ** 3).toFixed(1)} GB
				</div>
				{data.docker_info.images.slice(0, 5).map((img) => (
					<div key={img.id} className="ml-2">
						• {img.repo_tags[0] ?? img.id.slice(0, 12)} –{" "}
						{(img.size / 1024 ** 2).toFixed(1)} MB
					</div>
				))}
			</div>
		</div>
	);
}
