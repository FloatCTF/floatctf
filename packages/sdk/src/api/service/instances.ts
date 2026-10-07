import { type QueryParams, type UniResponse, service_api } from "../../transport.js";
import type { Instances, InstancesDto } from "../../types/instanceDto.js";

export type { Instances, InstancesDto };

export const instanceServiceApi = {
    launch: async (id: string): Promise<UniResponse<Instances>> => {
        const res = await service_api.post("/instances/launch", {
            challenge_id: id,
        });
        return res.data;
    },
    launchSingle: async (
        challenge_id: string,
        event_id: string,
    ): Promise<UniResponse<Instances>> => {
        const res = await service_api.post("/instances/launch", {
            challenge_id,
            event_id,
        });
        return res.data;
    },
    fetch: async (
        params: QueryParams = {},
    ): Promise<UniResponse<Instances[]>> => {
        const res = await service_api.get("/instances", { params });
        return res.data;
    },
    destroy: async (id: string): Promise<UniResponse<number>> => {
        const res = await service_api.delete(`/instances/${id}`);
        return res.data;
    },
    /** 批量删除（复选框选中后）：挑战实例销毁 / AWDP 实例停容器并移除行。 */
    bulkDelete: async (id_list: string[]): Promise<UniResponse<number>> => {
        const res = await service_api.delete("/instances", { data: { id_list } });
        return res.data;
    },
};
