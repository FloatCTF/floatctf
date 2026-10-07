import type { FloatCTFHttpClient } from "../../transport.js";
import type { QueryParams, UniResponse } from "../../transport.js";
import type { InstancesDto } from "../service/instances.js";

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

export function createInstanceAdminApi(http: FloatCTFHttpClient) {
	return {
		/** 某赛事的归一化实例列表（admin 赛事 Instance Tab）。 */
		listForEvent: async (
			eventId: string,
			params: QueryParams = {},
		): Promise<UniResponse<AdminInstanceRow[]>> => {
			const res = await http.get(`/events/${eventId}/instances`, { params });
			return res.data;
		},
};
}

export type InstanceAdminApi = ReturnType<typeof createInstanceAdminApi>;

export type { InstancesDto as Instances };
