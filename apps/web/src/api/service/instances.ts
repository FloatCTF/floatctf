import { type QueryParams, type UniResponse, service_api } from "../axios";

/** 选手侧挑战实例 DTO（GET /api/instances，与后端 InstancesDto 对齐）。
 * 归一化后为合并列表：challenge（挑战练习）与 gamebox（AWDP 练习）两类。 */
export type InstancesDto = {
	id: string;
	status: string;
	flag: string;
	content: string | null;
	/** 关联挑战；AWDP 练习实例为 null（用 run_id/gamebox_id 标识）。 */
	challenge_id: string | null;
	event_id: string;
	team_id?: string | null;
	user_id: string;
	identifier: string;
	created_at: string;
	updated_at: string;
	destroy_at: string | null;
	challenge_title?: string | null;
	event_title?: string | null;
	user_name?: string | null;
	run_id?: string | null;
	gamebox_id?: string | null;
	gamebox_title?: string | null;
};

/** 兼容旧引用（实体型已拆分为 event_challenge_instance，DTO 字段才是接口返回）。 */
export type Instances = InstancesDto;

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
