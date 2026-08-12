import { type QueryParams, type UniResponse, admin_api } from "@/api/axios";
import type { InstancesDto } from "@/api/service/instances";

/**
 * 管理端统一实例条目（归一化视图）。
 * instance_type = "challenge"（jeopardy 挑战实例）| "gamebox"（AWD/AWDP GameBox 实例）。
 * content_title 为对应 title（challenge 名 / GameBox 名）。列表不返回 flag。
 */
export type AdminInstanceRow = {
	id: string;
	instance_type: "challenge" | "gamebox";
	status: string;
	identifier: string;
	event_id?: string | null;
	event_title?: string | null;
	user_id?: string | null;
	user_name?: string | null;
	team_id?: string | null;
	team_name?: string | null;
	content_title?: string | null;
	challenge_id?: string | null;
	gamebox_id?: string | null;
	runtime_generation?: number | null;
	created_at: string;
	updated_at: string;
	destroy_at?: string | null;
};

export const instanceAdminApi = {
	/** 某赛事的归一化实例列表（admin 赛事 Instance Tab）。 */
	listForEvent: async (
		eventId: string,
		params: QueryParams = {},
	): Promise<UniResponse<AdminInstanceRow[]>> => {
		const res = await admin_api.get(`/events/${eventId}/instances`, { params });
		return res.data;
	},
};

export type { InstancesDto as Instances };
