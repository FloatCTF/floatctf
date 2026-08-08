/**
 * AWD admin + player API clients (unified event routes).
 * Admin:  /api/admin/events/{eventId}/awd/...  (create: POST /api/admin/events/awd)
 * Player: /api/events/{eventId}/awd/...
 */
import { type UniResponse, admin_api, service_api } from "@/api/axios";

export type AwdGameBox = {
	id: string;
	team_id: string;
	template_id: string;
	status: string;
	gamebox_ip: string;
	container_name: string;
	health_status: string;
};

export type AwdScoreRow = {
	team_id: string;
	team_name: string;
	attack_score: number;
	defense_score: number;
	total_score: number;
	rank: number;
};

export type WireGuardConfigResponse = {
	config: string;
};

/** Admin AWD lifecycle (SuperAdmin). */
export const awdAdminApi = {
	createEvent: async (body: Record<string, unknown>): Promise<UniResponse<string>> => {
		const res = await admin_api.post("/events/awd", body);
		return res.data;
	},
	deploy: async (eventId: string): Promise<UniResponse<null>> => {
		const res = await admin_api.post(`/events/${eventId}/awd/deploy`);
		return res.data;
	},
	start: async (eventId: string): Promise<UniResponse<null>> => {
		const res = await admin_api.post(`/events/${eventId}/awd/start`);
		return res.data;
	},
	pause: async (eventId: string): Promise<UniResponse<null>> => {
		const res = await admin_api.post(`/events/${eventId}/awd/pause`);
		return res.data;
	},
	resume: async (eventId: string): Promise<UniResponse<null>> => {
		const res = await admin_api.post(`/events/${eventId}/awd/resume`);
		return res.data;
	},
	finish: async (eventId: string): Promise<UniResponse<null>> => {
		const res = await admin_api.post(`/events/${eventId}/awd/finish`);
		return res.data;
	},
	precheck: async (eventId: string): Promise<UniResponse<string>> => {
		const res = await admin_api.post(`/events/${eventId}/awd/precheck`);
		return res.data;
	},
	scores: async (eventId: string): Promise<UniResponse<AwdScoreRow[]>> => {
		const res = await admin_api.get(`/events/${eventId}/awd/scores`);
		return res.data;
	},
	archive: async (eventId: string): Promise<UniResponse<null>> => {
		const res = await admin_api.post(`/events/${eventId}/awd/archive`);
		return res.data;
	},
	resetGamebox: async (
		eventId: string,
		instanceId: string,
	): Promise<UniResponse<null>> => {
		const res = await admin_api.post(
			`/events/${eventId}/awd/gameboxes/${instanceId}/reset`,
		);
		return res.data;
	},
	/** P4-5/P4-6：AWD 跨层封禁（WG suspend + banned set reconcile + conntrack + publish）。 */
	banTeam: async (
		eventId: string,
		teamId: string,
		body: { reason?: string; durationSecs?: number },
	): Promise<UniResponse<string>> => {
		const res = await admin_api.post(
			`/events/${eventId}/awd/teams/${teamId}/ban`,
			{
				reason: body.reason,
				duration_secs: body.durationSecs,
			},
		);
		return res.data;
	},
	/** P4-5：AWD 解封（反向闭环：DB unbanned → WG 恢复 peers → banned set reconcile）。 */
	unbanTeam: async (
		eventId: string,
		teamId: string,
	): Promise<UniResponse<null>> => {
		const res = await admin_api.delete(
			`/events/${eventId}/awd/teams/${teamId}/ban`,
		);
		return res.data;
	},
	/** P3-10：内部 token 轮换（key_version+1 + 容器 rollout + 审计）。 */
	rotateTokens: async (eventId: string): Promise<UniResponse<null>> => {
		const res = await admin_api.post(`/events/${eventId}/awd/tokens/rotate`);
		return res.data;
	},
	/** P5-11：AWD 分数调整（审计）。 */
	adjustScore: async (
		eventId: string,
		body: { team_id: string; delta: number; reason: string },
	): Promise<UniResponse<null>> => {
		const res = await admin_api.post(
			`/events/${eventId}/awd/score/adjust`,
			body,
		);
		return res.data;
	},
};

/** Player AWD endpoints (User JWT). */
export const awdPlayerApi = {
	gameboxes: async (eventId: string): Promise<UniResponse<AwdGameBox[]>> => {
		const res = await service_api.get(`/events/${eventId}/awd/gameboxes`);
		return res.data;
	},
	resetGamebox: async (
		eventId: string,
		instanceId: string,
	): Promise<UniResponse<null>> => {
		const res = await service_api.post(
			`/events/${eventId}/awd/gameboxes/${instanceId}/reset`,
		);
		return res.data;
	},
	submitFlag: async (
		eventId: string,
		flag: string,
	): Promise<UniResponse<null>> => {
		const res = await service_api.post(`/events/${eventId}/awd/submissions`, {
			flag,
		});
		return res.data;
	},
	scores: async (eventId: string): Promise<UniResponse<AwdScoreRow[]>> => {
		const res = await service_api.get(`/events/${eventId}/awd/scores`);
		return res.data;
	},
	wireguardConfig: async (
		eventId: string,
	): Promise<UniResponse<WireGuardConfigResponse>> => {
		const res = await service_api.get(
			`/events/${eventId}/awd/wireguard/config`,
		);
		return res.data;
	},
};
