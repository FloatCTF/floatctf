/** 磁盘信息。 */
export type DiskInformation = {
	name: string;
	mount_point: string;
	file_system: string;
	total_space: number;
	available_space: number;
	used_space: number;
	usage_percent: number;
};

/** 网卡信息。 */
export type NetworkInterfaceInfo = {
	name: string;
	ip_addresses: string[];
	received: number;
	transmitted: number;
	recv_rate: number;
	transmit_rate: number;
};

/** Docker 镜像摘要。 */
export type DockerImageInfo = {
	id: string;
	repo_tags: string[];
	size: number;
};

/** Docker 概况。 */
export type DockerInformation = {
	image_count: number;
	images: DockerImageInfo[];
	running_container_count: number;
	total_disk: number;
};

/** 管理端仪表盘系统信息。 */
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
