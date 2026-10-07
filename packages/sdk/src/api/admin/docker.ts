import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";

export interface FloatDockerContainer {
	id: string;
	name: string;
	status: string;
	image: string;
	ports: string;
	created: number;
}

export interface ContainerInfo {
	id: string;
	names: string[];
	image: string;
	image_id: string;
	state: string;
	status: string;
	created: number;
	ports: PortInfo[];
}

export interface PortInfo {
	IP?: string;
	PrivatePort: number;
	PublicPort?: number;
	Type: string;
}

export interface ImageInfo {
	id: string;
	repo_tags: string[];
	size: number;
	created: number;
}

export interface NetworkInfo {
	id: string;
	name: string;
	driver: string;
	scope: string;
	ipam_driver: string;
	subnet?: string;
	gateway?: string;
	created: number;
}

export function createDockerAdminApi(http: FloatCTFHttpClient) {
	return {
		fetchContainers: async (
			params: QueryParams = {},
		): Promise<UniResponse<FloatDockerContainer[]>> => {
			const res = await http.get("/docker/containers", { params });
			return res.data;
		},
		stopContainer: async (container_id: string): Promise<UniResponse<null>> => {
			const res = await http.post(`/docker/containers/${container_id}/stop`);
			return res.data;
		},
		startContainer: async (container_id: string): Promise<UniResponse<null>> => {
			const res = await http.post(
				`/docker/containers/${container_id}/start`,
			);
			return res.data;
		},
		deleteContainer: async (container_id: string): Promise<UniResponse<null>> => {
			const res = await http.delete(`/docker/containers/${container_id}`);
			return res.data;
		},
		fetchImages: async (
			params: QueryParams = {},
		): Promise<UniResponse<ImageInfo[]>> => {
			const res = await http.get("/docker/images", { params });
			return res.data;
		},
		deleteImage: async (image_id: string): Promise<UniResponse<null>> => {
			const res = await http.delete(`/docker/images/${image_id}`);
			return res.data;
		},
		fetchNetworks: async (
			params: QueryParams = {},
		): Promise<UniResponse<NetworkInfo[]>> => {
			const res = await http.get("/docker/networks", { params });
			return res.data;
		},
		createNetwork: async (network: {
			name: string;
			subnet: string;
			gateway: string;
			driver?: string;
		}): Promise<UniResponse<NetworkInfo>> => {
			const res = await http.post("/docker/networks", network);
			return res.data;
		},
		deleteNetwork: async (network_id: string): Promise<UniResponse<null>> => {
			const res = await http.delete(`/docker/networks/${network_id}`);
			return res.data;
		},
};
}

export type DockerAdminApi = ReturnType<typeof createDockerAdminApi>;
