/**
 * 选手侧挑战/靶机实例 DTO（`GET /api/instances`，与后端 `InstancesDto` 对齐）。
 *
 * 归一化后为合并列表：challenge（挑战练习）与 gamebox（AWDP 练习）两类。
 * **注意**：这不是生成的实体 `entity/event_challenge_instance`，字段以接口返回为准。
 */
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

/** 兼容旧引用（实体型已拆分为 entity/event_challenge_instance，DTO 字段才是接口返回）。 */
export type Instances = InstancesDto;
