/**
 * AWD admin + player API clients (unified event routes).
 * Admin:  /api/admin/events/{eventId}/awd/...  (create: POST /api/admin/events/awd)
 * Player: /api/events/{eventId}/awd/...
 */
import { type QueryParams, type UniResponse, admin_api, service_api } from "@/api/axios";

export type AwdGameBox = {
	id: string;
	team_id: string;
	event_gamebox_id: string;
	status: string;
	gamebox_ip: string;
	container_name: string;
	health_status: string;
};

export type GameBoxRevisionDto = {
	id: string;
	revision_number: number;
	image_ref: string;
	image_digest: string | null;
	username: string;
	spec_digest: string;
	// 完整配置回传（Edit 对话框回填，§46）
	cpu_millis: number;
	memory_bytes: number;
	pids_limit: number;
	healthcheck_json: Record<string, unknown> | null;
	judge_script_name: string | null;
	judge_script_content: string | null;
	judge_args_json: Record<string, unknown> | null;
	judge_timeout_secs: number | null;
	judge_retry_interval_secs: number | null;
	created_at: string;
};

export type GameBoxLibraryDto = {
	id: string;
	name: string;
	safe_name: string;
	category: string;
	description: string;
	hidden: boolean;
	latest_revision: GameBoxRevisionDto | null;
};

export type EventGameBoxDto = {
	id: string;
	gamebox_id: string;
	gamebox_name: string;
	gamebox_safe_name: string;
	revision_id: string;
	revision_number: number;
	host_offset: number;
	enabled: boolean;
	hidden: boolean;
	cpu_millis: number;
	memory_bytes: number;
	pids_limit: number;
	judge_timeout_secs: number | null;
	judge_retry_interval_secs: number | null;
	break_points: number;
	loss_points: number;
	fix_points: number;
	down_points: number;
	first_bonus: number;
	created_at: string;
};

export type GameBoxConfigPayload = {
	source_toml?: string;
	image_ref: string;
	image_digest?: string | null;
	username?: string;
	cpu_millis?: number;
	memory_bytes?: number;
	pids_limit?: number;
	healthcheck?: Record<string, unknown> | null;
	judge_script_name?: string | null;
	judge_script_content?: string | null;
	judge_args?: Record<string, unknown> | null;
	judge_timeout_secs?: number | null;
	judge_retry_interval_secs?: number | null;
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
	// ── GameBox 库（全局 identity + immutable Revision，§46）──
	// 支持 Challenges 同款搜索/分页：?page=&limit=&filter=name:xx&category:yy
	listGameboxes: async (
		params: QueryParams = {},
	): Promise<UniResponse<GameBoxLibraryDto[]>> => {
		const res = await admin_api.get(`/awd/gameboxes`, { params });
		return res.data;
	},
	createGamebox: async (
		body: {
			name: string;
			safe_name?: string;
			category?: string;
			description?: string;
			hidden?: boolean;
			config: GameBoxConfigPayload;
		},
	): Promise<UniResponse<GameBoxLibraryDto>> => {
		const res = await admin_api.post(`/awd/gameboxes`, body);
		return res.data;
	},
	editGameboxRevision: async (
		gameboxId: string,
		body: { config: GameBoxConfigPayload },
	): Promise<UniResponse<GameBoxLibraryDto>> => {
		const res = await admin_api.post(
			`/awd/gameboxes/${gameboxId}/revisions`,
			body,
		);
		return res.data;
	},
	hideGamebox: async (gameboxId: string): Promise<UniResponse<null>> => {
		const res = await admin_api.post(`/awd/gameboxes/${gameboxId}/hide`);
		return res.data;
	},
	// ── 赛事 GameBox 选择（EventGameBox）──
	listEventGameboxes: async (
		eventId: string,
		params: QueryParams = {},
	): Promise<UniResponse<EventGameBoxDto[]>> => {
		const res = await admin_api.get(`/events/${eventId}/awd/gameboxes`, {
			params,
		});
		return res.data;
	},
	addEventGamebox: async (
		eventId: string,
		body: {
			gamebox_id: string;
			revision_id?: string;
			host_offset?: number;
			hidden?: boolean;
			break_points?: number;
			loss_points?: number;
			fix_points?: number;
			down_points?: number;
			first_bonus?: number;
		},
	): Promise<UniResponse<EventGameBoxDto>> => {
		const res = await admin_api.post(
			`/events/${eventId}/awd/gameboxes`,
			body,
		);
		return res.data;
	},
	updateEventGamebox: async (
		eventId: string,
		eventGameboxId: string,
		body: {
			revision_id?: string;
			enabled?: boolean;
			hidden?: boolean;
			cpu_millis?: number;
			memory_bytes?: number;
			pids_limit?: number;
			judge_timeout_secs?: number | null;
			judge_retry_interval_secs?: number | null;
			break_points?: number;
			loss_points?: number;
			fix_points?: number;
			down_points?: number;
			first_bonus?: number;
		},
	): Promise<UniResponse<EventGameBoxDto>> => {
		const res = await admin_api.patch(
			`/events/${eventId}/awd/gameboxes/${eventGameboxId}`,
			body,
		);
		return res.data;
	},
	removeEventGamebox: async (
		eventId: string,
		eventGameboxId: string,
	): Promise<UniResponse<null>> => {
		const res = await admin_api.delete(
			`/events/${eventId}/awd/gameboxes/${eventGameboxId}`,
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
