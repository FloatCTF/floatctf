/**
 * AWD 管理端 + 选手端 API 客户端（统一赛事路由）。
 * 管理端：/api/admin/events/{eventId}/awd/...（创建：POST /api/admin/events/awd）
 * 选手端：/api/events/{eventId}/awd/...
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
	round_count: number | null;
	round_duration_secs: number;
	initial_score: number;
	free_reset_count: number;
	extra_reset_penalty: number;
	judge_max_concurrency: number;
	judge_default_timeout_secs: number;
	judge_retry_interval_secs: number;
	archive_retention_hours: number;
	planned_start_at: string | null;
	verified_at: string | null;
	started_at: string | null;
	updated_at: string;
};

export type AwdEventConfigInput = {
	/** PATCH 乐观锁版本；首次创建可省略。 */
	expected_updated_at?: string;
	round_count?: number;
	round_duration_secs?: number;
	initial_score?: number;
	free_reset_count?: number;
	extra_reset_penalty?: number;
	judge_max_concurrency?: number;
	judge_default_timeout_secs?: number;
	judge_retry_interval_secs?: number;
	archive_retention_hours?: number;
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

export type GameBoxLibraryDto = {
	id: string;
	name: string;
	safe_name: string;
	category: string;
	description: string;
	hidden: boolean;
	version: string | null;
	build_status: string | null;
	package_digest: string | null;
	image_ref: string | null;
	image_repo_digest: string | null;
	username: string | null;
	cpu_millis: number | null;
	memory_bytes: number | null;
	pids_limit: number | null;
	healthchecks_json: unknown | null;
	judge_script_name: string | null;
	judge_script_content: string | null;
	judge_args_json: unknown | null;
	judge_timeout_secs: number | null;
	judge_retry_interval_secs: number | null;
};

export type ImportGameBoxResponse = {
	gamebox: GameBoxLibraryDto;
};

/** POST /awd/gameboxes/scan 结果行。 */
export type GameBoxScanItem = {
	safe_name: string;
	name: string | null;
	version: string | null;
	status: "added" | "skipped" | "error";
	message: string;
};

/** POST /awd/gameboxes/check 结果行。 */
export type GameBoxCheckResult = {
	id: string;
	gamebox_name: string;
	is_ok: boolean;
	docker_image: boolean;
	package_dir: boolean;
};

/** POST /awd/gameboxes/build 结果行。 */
export type GameBoxBuildResult = {
	gamebox_name: string;
	is_ok: boolean;
	message: string;
};

export type EventGameBoxDto = {
	id: string;
	gamebox_id: string;
	gamebox_name: string;
	gamebox_safe_name: string;
	gamebox_version: string | null;
	host_offset: number;
	enabled: boolean;
	hidden: boolean;
	cpu_millis: number;
	memory_bytes: number;
	pids_limit: number;
	judge_timeout_secs: number | null;
	judge_retry_interval_secs: number | null;
	attack_score: number;
	judge_down_penalty: number;
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

/** 选手端 AWD 赛事状态（GET /api/events/{event_id}/awd/status）。 */
export type AwdPlayerStatus = {
	event_id: string;
	status: string;
	phase: string;
	current_round: number | null;
	round_count: number | null;
	banned: boolean;
	score: number | null;
};

export type WireGuardConfigResponse = {
	config: string;
};

// ── AWD 网络控制面（§4-§7 / §22-§24 / §64-§67 / §73）──

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

/** 管理端 AWD 生命周期（SuperAdmin）。 */
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
	/** 手动封禁队伍（无定时，需管理员手动解封）。 */
	banTeam: async (
		eventId: string,
		teamId: string,
		body: { reason?: string },
	): Promise<UniResponse<string>> => {
		const res = await admin_api.post(
			`/events/${eventId}/awd/teams/${teamId}/ban`,
			{
				reason: body.reason,
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
	/** POST multipart 字段 `package_zip`——同步构建。 */
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
			username?: string | null;
			recommended_cpu_millis?: number | null;
			recommended_memory_bytes?: number | null;
			recommended_pids_limit?: number | null;
			/** JSON 文本；null 清空。 */
			healthchecks_json?: string | null;
			judge_script_name?: string | null;
			judge_script_content?: string | null;
			/** JSON 文本；null 清空。 */
			judge_args_json?: string | null;
			judge_timeout_secs?: number | null;
			judge_retry_interval_secs?: number | null;
		},
	): Promise<UniResponse<GameBoxLibraryDto>> => {
		const res = await admin_api.patch(`/awd/gameboxes/${gameboxId}`, body);
		return res.data;
	},
	hideGamebox: async (gameboxId: string): Promise<UniResponse<null>> => {
		const res = await admin_api.post(`/awd/gameboxes/${gameboxId}/hide`);
		return res.data;
	},
	// 批量删除（仿 challenges.remove）：仅未被赛事 / AWDP Run 引用的可删
	removeGamebox: async (id_list: string[]): Promise<UniResponse<number>> => {
		const res = await admin_api.delete(`/awd/gameboxes`, {
			data: { id_list },
		});
		return res.data;
	},
	scanGameboxes: async (): Promise<UniResponse<GameBoxScanItem[]>> => {
		const res = await admin_api.post(`/awd/gameboxes/scan`);
		return res.data;
	},
	checkGameboxes: async (
		gamebox_id_list?: string[],
	): Promise<UniResponse<GameBoxCheckResult[]>> => {
		const res = await admin_api.post("/awd/gameboxes/check", {
			gamebox_id_list,
		});
		return res.data;
	},
	buildGameboxes: async (
		gamebox_id_list?: string[],
	): Promise<UniResponse<GameBoxBuildResult[]>> => {
		const res = await admin_api.post("/awd/gameboxes/build", {
			gamebox_id_list,
		});
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
	// ── 赛事网络（§22/§64）──
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
			gamebox_id: string;
			host_offset?: number;
			hidden?: boolean;
			attack_score?: number;
			judge_down_penalty?: number;
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
			attack_score?: number;
			judge_down_penalty?: number;
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

/** 选手端 AWD 接口（用户 JWT）。 */
export const awdPlayerApi = {
	/** 获取 AWD 赛事状态（phase, round, ban state, score）。 */
	status: async (
		eventId: string,
	): Promise<UniResponse<AwdPlayerStatus>> => {
		const res = await service_api.get(`/events/${eventId}/awd/status`);
		return res.data;
	},
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
