import { ProgressBar, Spinner } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

import { adminApi } from "@/api";
import { AdminRouteGuard } from "@/routes/admin/route";

export const Route = createFileRoute("/admin/dashboard")({
	component: RouteComponent,
	loader: AdminRouteGuard,
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
function RouteComponent() {
	const {
		data: d,
		isLoading,
		isError,
	} = useQuery({
		queryKey: ["system_information"],
		queryFn: adminApi.system.monitor,
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
			{/* Top Section: System Info + Resources */}
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
