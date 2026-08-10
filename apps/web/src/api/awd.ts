/**
 * AWD admin + player API clients (unified event routes).
 * Admin:  /api/admin/events/{eventId}/awd/...  (create: POST /api/admin/events/awd)
 * Player: /api/events/{eventId}/awd/...
 */
import {
	type QueryParams,
	type UniResponse,
	admin_api,
	service_api,
} from "@/api/axios";

export type AwdEventStatus = {
	event_id: string;
	status: string;
	phase: string;
	round_duration_secs: number;
	free_reset_count: number;
	extra_reset_penalty: number;
	reset_protection_secs: number;
	judge_max_concurrency: number;
	judge_default_timeout_secs: number;
	judge_retry_interval_secs: number;
	judge_grace_period_secs: number;
	archive_retention_hours: number;
	planned_start_at: string | null;
	verified_at: string | null;
	started_at: string | null;
	updated_at: string;
};

export type AwdEventConfigInput = {
	/** PATCH optimistic-lock version; omitted on first create. */
	expected_updated_at?: string;
	round_duration_secs: number;
	free_reset_count: number;
	extra_reset_penalty: number;
	reset_protection_secs: number;
	judge_max_concurrency: number;
	judge_default_timeout_secs: number;
	judge_retry_interval_secs: number;
	judge_grace_period_secs: number;
	archive_retention_hours: number;
	planned_start_at?: string;
	clear_planned_start?: boolean;
};

export type AwdGameBox = {
	id: string;
	team_id: string;
	event_gamebox_id: string;
	gamebox_name: string;
	status: string;
	gamebox_ip: string;
	container_name: string;
	health_status: string;
};

export type GameBoxRevisionSummaryDto = {
	id: string;
	version: string;
	revision_number: number;
	build_status: string;
	package_digest: string;
	image_ref: string | null;
	image_id: string | null;
	image_repo_digest: string | null;
	username: string;
	recommended_cpu_millis: number;
	recommended_memory_bytes: number;
	recommended_pids_limit: number;
	healthchecks_json: unknown;
	judge_script_name: string | null;
	judge_timeout_secs: number | null;
	judge_retry_interval_secs: number | null;
	build_error: string | null;
	created_at: string;
};

export type GameBoxLibraryDto = {
	id: string;
	name: string;
	safe_name: string;
	category: string;
	description: string;
	hidden: boolean;
	latest_revision: GameBoxRevisionSummaryDto | null;
	// projected from latest_revision for list convenience
	image_ref: string | null;
	image_repo_digest: string | null;
	username: string | null;
	cpu_millis: number | null;
	memory_bytes: number | null;
	pids_limit: number | null;
	healthchecks_json: unknown | null;
	build_status: string | null;
	version: string | null;
	package_digest: string | null;
};

export type ImportGameBoxResponse = {
	gamebox: GameBoxLibraryDto;
	revision: GameBoxRevisionSummaryDto;
	already_exists: boolean;
};

export type EventGameBoxDto = {
	id: string;
	gamebox_id: string;
	gamebox_revision_id: string;
	gamebox_name: string;
	gamebox_safe_name: string;
	revision_version: string;
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

/** @deprecated Manual create-with-config removed; use package import. */
export type GameBoxConfigPayload = {
	name?: string;
	category?: string;
	description?: string;
	hidden?: boolean;
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

// ── AWD Network Control Plane（§4-§7 / §22-§24 / §64-§67 / §73）──

/** 平台网络设置 + 容量预览（GET /admin/awd/network）。 */
export type PlatformNetworkSettings = {
	gamebox_pool: string;
	gamebox_event_prefix: number;
	gamebox_team_prefix: number;
	wireguard_pool: string;
	wireguard_event_prefix: number;
	wireguard_team_prefix: number;
	wireguard_port_min: number;
	wireguard_port_max: number;
	wireguard_public_endpoint: string | null;
	updated_at: string;
	// 容量预览（§67，来自 GET 计算）
	gamebox_event_capacity: number;
	gamebox_team_capacity_per_event: number;
	gamebox_hosts_per_team: number;
	wireguard_event_capacity: number;
	wireguard_team_capacity_per_event: number;
	wireguard_port_capacity: number;
};

/** PATCH /admin/awd/network 的请求体（全部可选，部分更新）。 */
export type PlatformNetworkSettingsUpdate = {
	gamebox_pool?: string;
	gamebox_event_prefix?: number;
	gamebox_team_prefix?: number;
	wireguard_pool?: string;
	wireguard_event_prefix?: number;
	wireguard_team_prefix?: number;
	wireguard_port_min?: number;
	wireguard_port_max?: number;
	wireguard_public_endpoint?: string | null;
};

/** PATCH 响应：返回更新的少量字段（含 note）。 */
export type PlatformNetworkSettingsUpdateResponse = Partial<
	Pick<
		PlatformNetworkSettings,
		| "gamebox_pool"
		| "wireguard_pool"
		| "wireguard_public_endpoint"
		| "updated_at"
	>
> & { note?: string };

/** Host 观测状态（§4.1，纯只读）。 */
export type PlatformNetworkHealth = {
	nftables: string;
	wireguard: string;
	docker: string;
	firewall_runtime: string;
	floatctf_table: string;
	docker_firewall_backend: string | null;
	firewalld: string;
	ipv4_forwarding: string | null;
	ipv6_policy: string;
	capability_supported: boolean;
	notes: string[];
};

/** 平台分配账本行（§7/§66）。 */
export type PlatformNetworkAllocation = {
	event_id: string;
	event_title: string | null;
	kind: string;
	cidr: string;
	allocated_at: string;
	released_at: string | null;
	active: boolean;
};

/** Event Network（§22/§64）：未分配时 GET 返回 404（data=null）。 */
export type EventNetworkInfo = {
	event_id: string;
	allocation_mode: string;
	gamebox_cidr: string;
	wireguard_cidr: string;
	infrastructure_subnet: string;
	flagserver_ip: string;
	judgeserver_ip: string;
	wireguard_interface_name: string;
	wireguard_listen_port: number;
	docker_network_name: string;
	locked: boolean;
};

/** PUT /events/{eventId}/awd/network 请求体（automatic 默认；manual 需两个 CIDR）。 */
export type NetworkAllocationRequest = {
	allocation_mode?: "automatic" | "manual";
	gamebox_cidr?: string;
	wireguard_cidr?: string;
	wireguard_listen_port?: number;
};

/** Admin AWD lifecycle (SuperAdmin). */
export const awdAdminApi = {
	getStatus: async (
		eventId: string,
	): Promise<UniResponse<AwdEventStatus | null>> => {
		const res = await admin_api.get(`/events/${eventId}/awd`);
		return res.data;
	},
	createEvent: async (
		body: AwdEventConfigInput & { event_id: string },
	): Promise<UniResponse<string>> => {
		const res = await admin_api.post("/events/awd", body);
		return res.data;
	},
	updateConfig: async (
		eventId: string,
		body: AwdEventConfigInput,
	): Promise<UniResponse<AwdEventStatus>> => {
		const res = await admin_api.patch(`/events/${eventId}/awd`, body);
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
	// ── GameBox 库（identity + revisions；package import）──
	listGameboxes: async (
		params: QueryParams = {},
	): Promise<UniResponse<GameBoxLibraryDto[]>> => {
		const res = await admin_api.get(`/awd/gameboxes`, { params });
		return res.data;
	},
	/** POST multipart field `package_zip` — synchronous build. */
	importGamebox: async (
		file: File | Blob,
	): Promise<UniResponse<ImportGameBoxResponse>> => {
		const form = new FormData();
		form.append("package_zip", file);
		const res = await admin_api.post(`/awd/gameboxes/import`, form, {
			headers: { "Content-Type": "multipart/form-data" },
		});
		return res.data;
	},
	updateGamebox: async (
		gameboxId: string,
		body: {
			name?: string;
			category?: string;
			description?: string;
			hidden?: boolean;
		},
	): Promise<UniResponse<GameBoxLibraryDto>> => {
		const res = await admin_api.patch(`/awd/gameboxes/${gameboxId}`, body);
		return res.data;
	},
	hideGamebox: async (gameboxId: string): Promise<UniResponse<null>> => {
		const res = await admin_api.post(`/awd/gameboxes/${gameboxId}/hide`);
		return res.data;
	},
	// ── 平台网络（Control Plane，§73）──
	getPlatformNetwork: async (): Promise<
		UniResponse<PlatformNetworkSettings>
	> => {
		const res = await admin_api.get(`/awd/network`);
		return res.data;
	},
	updatePlatformNetwork: async (
		body: PlatformNetworkSettingsUpdate,
	): Promise<UniResponse<PlatformNetworkSettingsUpdateResponse>> => {
		const res = await admin_api.patch(`/awd/network`, body);
		return res.data;
	},
	/** §4.1 Host 观测状态（纯只读）。 */
	getPlatformNetworkHealth: async (): Promise<
		UniResponse<PlatformNetworkHealth>
	> => {
		const res = await admin_api.get(`/awd/network/health`);
		return res.data;
	},
	/** §7/§66 平台分配账本（只读）。 */
	getPlatformNetworkAllocations: async (): Promise<
		UniResponse<PlatformNetworkAllocation[]>
	> => {
		const res = await admin_api.get(`/awd/network/allocations`);
		return res.data;
	},
	// ── Event Network（§22/§64）──
	/** 未分配时后端返回 404（data=null）。 */
	getEventNetwork: async (
		eventId: string,
	): Promise<UniResponse<EventNetworkInfo>> => {
		const res = await admin_api.get(`/events/${eventId}/awd/network`);
		return res.data;
	},
	/** PUT 分配：无 body 即 automatic；manual 需 gamebox_cidr + wireguard_cidr。 */
	allocateEventNetwork: async (
		eventId: string,
		body: NetworkAllocationRequest,
	): Promise<UniResponse<null>> => {
		const res = await admin_api.put(`/events/${eventId}/awd/network`, body);
		return res.data;
	},
	/** §33/§93 重新分配（仅未锁定）。 */
	reallocateEventNetwork: async (
		eventId: string,
	): Promise<UniResponse<null>> => {
		const res = await admin_api.post(
			`/events/${eventId}/awd/network/reallocate`,
		);
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
			gamebox_revision_id?: string;
			gamebox_id?: string;
			version?: string;
			host_offset?: number;
			hidden?: boolean;
			break_points?: number;
			loss_points?: number;
			fix_points?: number;
			down_points?: number;
			first_bonus?: number;
		},
	): Promise<UniResponse<EventGameBoxDto>> => {
		const res = await admin_api.post(`/events/${eventId}/awd/gameboxes`, body);
		return res.data;
	},
	updateEventGamebox: async (
		eventId: string,
		eventGameboxId: string,
		body: {
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
	/**
	 * 队伍级 SSH 访问凭据（GET /events/{eventId}/awd/ssh-config）。
	 */
	sshConfig: async (
		eventId: string,
	): Promise<UniResponse<SshAccessResponse>> => {
		const res = await service_api.get(`/events/${eventId}/awd/ssh-config`);
		return res.data;
	},
};

export type SshInstanceInfo = {
	id: string;
	gamebox_ip: string;
	username: string;
	container_name: string;
	health_status: string;
};

export type SshAccessResponse = {
	port: number;
	password: string;
	instances: SshInstanceInfo[];
};
