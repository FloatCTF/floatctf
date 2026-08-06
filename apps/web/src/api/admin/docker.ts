import { type QueryParams, type UniResponse, admin_api } from "@/api/axios";

export interface FloatDockerContainer {
    id: string;
    name: string;
    challenge_name?: string;
    event_name?: string;
    net_info: string;
    status: string;
    image_name: string;
    uptime: string;
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

export const dockerAdminApi = {
    fetchContainers: async (
        params: QueryParams = {},
    ): Promise<UniResponse<FloatDockerContainer[]>> => {
        const res = await admin_api.get("/docker/containers", { params });
        return res.data;
    },
    getContainer: async (
        container_id: string,
    ): Promise<UniResponse<ContainerInfo>> => {
        const res = await admin_api.get(`/docker/containers/${container_id}`);
        return res.data;
    },
    stopContainer: async (container_id: string): Promise<UniResponse<null>> => {
        const res = await admin_api.post(
            `/docker/containers/${container_id}/stop`,
        );
        return res.data;
    },
    startContainer: async (
        container_id: string,
    ): Promise<UniResponse<null>> => {
        const res = await admin_api.post(
            `/docker/containers/${container_id}/start`,
        );
        return res.data;
    },
    deleteContainer: async (
        container_id: string,
    ): Promise<UniResponse<null>> => {
        const res = await admin_api.delete(
            `/docker/containers/${container_id}`,
        );
        return res.data;
    },
    fetchImages: async (
        params: QueryParams = {},
    ): Promise<UniResponse<ImageInfo[]>> => {
        const res = await admin_api.get("/docker/images", { params });
        return res.data;
    },
    deleteImage: async (image_id: string): Promise<UniResponse<null>> => {
        const res = await admin_api.delete(`/docker/images/${image_id}`);
        return res.data;
    },
    fetchNetworks: async (
        params: QueryParams = {},
    ): Promise<UniResponse<NetworkInfo[]>> => {
        const res = await admin_api.get("/docker/networks", { params });
        return res.data;
    },
    createNetwork: async (network: {
        name: string;
        subnet: string;
        gateway: string;
        driver?: string;
    }): Promise<UniResponse<NetworkInfo>> => {
        const res = await admin_api.post("/docker/networks", network);
        return res.data;
    },
    deleteNetwork: async (network_id: string): Promise<UniResponse<null>> => {
        const res = await admin_api.delete(`/docker/networks/${network_id}`);
        return res.data;
    },
};
