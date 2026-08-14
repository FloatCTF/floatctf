/**
 * AWDP 管理端 + 选手端 API 客户端。
 * 管理端：/api/admin/events/{eventId}/awdp/...
 * 选手端：/api/events/{eventId}/awdp/...
 */
import {
	type QueryParams,
	type UniResponse,
	admin_api,
	service_api,
} from "@/api/axios";

// ────────────────────────────────────────────────────────────────────────────
// Types
// ────────────────────────────────────────────────────────────────────────────

export type AwdpPhase = "pending" | "break" | "fix" | "ended";

export type AwdpEndpoint = {
	protocol: "http" | "tcp";
	container_port: number;
	public_host: string;
	public_port: number;
};

export type AwdpInstance = {
	instance_id: string;
	runtime_state: string;
	runtime_generation: number;
	/** 玩家手动 Reset 次数（比赛 subject×gamebox；练习恒 0）。 */
	reset_count: number;
	endpoints: AwdpEndpoint[];
};

export type AwdpGameBox = {
	id: string;
	gamebox_id: string;
	name: string;
	category: string;
	enabled: boolean;
	hidden: boolean;
	exposed: [string, number][];
	broken: boolean;
	instance: AwdpInstance | null;
	source_code_dir?: string | null;
};

export type AwdpOverview = {
	event_id: string;
	phase: AwdpPhase;
	break_duration_secs: number;
	fix_duration_secs: number;
	fix_round_interval_secs: number;
	total_rounds: number;
	break_score: number;
	fix_round_score: number;
	started_at: string | null;
	break_ends_at: string | null;
	fix_started_at: string | null;
	fix_ends_at: string | null;
	finished_at: string | null;
	current_round: number;
	next_action_at: string | null;
	my_score: number;
	gameboxes: AwdpGameBox[];
};

export type BreakSubmitResponse = {
	accepted: boolean;
	scored: boolean;
	already_broken: boolean;
};

export type PatchSubmitResponse = {
	status: "applied" | "failed";
	error_message?: string | null;
};

export type ManualCheckDto = {
	/** 创建的 manual 评估 id（前端据 id 从 evaluations 列表取终态详情）。 */
	evaluation_id: string;
	/** 初始状态（"pending"）；终态用 evaluations 行的 healthcheck/judge 结果。 */
	status: string;
	healthcheck_ok: boolean | null;
	healthcheck_detail: string[] | null;
	judge_ok: boolean | null;
	judge_detail: string | null;
	/** 练习模式才执行 exploit 诊断（不计分）；非练习恒 null。 */
	exploit_ok: boolean | null;
	exploit_detail: string | null;
};

/** ALL Check 结果（练习模式；status=patched → 剩余回合全部计分 + run 已结束）。 */
export type AllCheckDto = {
	/** 终态：patched=修复成功（swept=true）；其余 = 本次未通过（不落账，等官方 check）。 */
	status: string;
	/** status=patched：剩余回合已全部计分且 run 已结束。 */
	swept: boolean;
	swept_rounds: number;
	target_round: number;
	healthcheck_detail: string | null;
	judge_detail: string | null;
	exploit_detail: string | null;
};

export type AwdpEventConfigDto = {
	event_id: string;
	phase: AwdpPhase;
	break_duration_secs: number;
	fix_duration_secs: number;
	fix_round_interval_secs: number;
	break_score: number;
	fix_round_score: number;
	total_rounds: number;
	configuration_generation: number;
	updated_at: string;
	started_at: string | null;
	break_ends_at: string | null;
	fix_started_at: string | null;
	fix_ends_at: string | null;
	finished_at: string | null;
	current_round: number;
	next_action_at: string | null;
};

export type AwdpConfigPatchInput = {
	expected_updated_at?: string;
	break_duration_secs?: number;
	fix_duration_secs?: number;
	fix_round_interval_secs?: number;
	break_score?: number;
	fix_round_score?: number;
};

export type AwdpAdminEventGameBoxDto = {
	id: string;
	event_id: string;
	gamebox_id: string;
	name: string;
	safe_name: string;
	category: string;
	enabled: boolean;
	hidden: boolean;
	cpu_millis: number;
	memory_bytes: number;
	pids_limit: number;
	awdp_capable: boolean;
	awdp_source_code_dir: string | null;
	build_status: string | null;
};

export type AwdpAdminInstanceDto = {
	instance_id: string;
	event_gamebox_id: string;
	gamebox_name: string;
	owner_user_id: string | null;
	owner_team_id: string | null;
	runtime_state: string;
	runtime_generation: number;
	container_name: string;
	endpoints: AwdpEndpoint[];
};

/** AWDP 赛事积分榜条目（个人或战队主体）。 */
export type AwdpScoreRow = {
	subject_id: string;
	subject_name: string;
	break_score: number;
	fix_score: number;
	total_score: number;
	rank: number;
};

// ────────────────────────────────────────────────────────────────────────────
// Admin client
// ────────────────────────────────────────────────────────────────────────────

export const awdpAdminApi = {
	getConfig: async (eventId: string) => {
		const res = await admin_api.get<UniResponse<AwdpEventConfigDto>>(
			`/events/${eventId}/awdp`,
		);
		return res.data;
	},
	updateConfig: async (eventId: string, body: AwdpConfigPatchInput) => {
		const res = await admin_api.patch<UniResponse<AwdpEventConfigDto>>(
			`/events/${eventId}/awdp`,
			body,
		);
		return res.data;
	},
	start: async (eventId: string) => {
		const res = await admin_api.post<UniResponse<null>>(
			`/events/${eventId}/awdp/start`,
		);
		return res.data;
	},
	breakToFix: async (eventId: string) => {
		const res = await admin_api.post<UniResponse<null>>(
			`/events/${eventId}/awdp/break-to-fix`,
		);
		return res.data;
	},
	finish: async (eventId: string) => {
		const res = await admin_api.post<UniResponse<null>>(
			`/events/${eventId}/awdp/finish`,
		);
		return res.data;
	},
	attachGamebox: async (
		eventId: string,
		gameboxId: string,
		hidden?: boolean,
	) => {
		const res = await admin_api.post<UniResponse<AwdpAdminEventGameBoxDto>>(
			`/events/${eventId}/awdp/gameboxes`,
			{
				gamebox_id: gameboxId,
				hidden,
			},
		);
		return res.data;
	},
	detachGamebox: async (eventId: string, egId: string) => {
		const res = await admin_api.delete<UniResponse<null>>(
			`/events/${eventId}/awdp/gameboxes/${egId}`,
		);
		return res.data;
	},
	listEventGameboxes: async (eventId: string, params?: QueryParams) => {
		const res = await admin_api.get<UniResponse<AwdpAdminEventGameBoxDto[]>>(
			`/events/${eventId}/awdp/gameboxes`,
			{ params },
		);
		return res.data;
	},
	listInstances: async (eventId: string) => {
		const res = await admin_api.get<UniResponse<AwdpAdminInstanceDto[]>>(
			`/events/${eventId}/awdp/instances`,
		);
		return res.data;
	},
	scores: async (eventId: string) => {
		const res = await admin_api.get<UniResponse<AwdpScoreRow[]>>(
			`/events/${eventId}/awdp/scores`,
		);
		return res.data;
	},
};

// ────────────────────────────────────────────────────────────────────────────
// Player client
// ────────────────────────────────────────────────────────────────────────────

export const awdpPlayerApi = {
	overview: async (eventId: string) => {
		const res = await service_api.get<UniResponse<AwdpOverview>>(
			`/events/${eventId}/awdp`,
		);
		return res.data;
	},
	startInstance: async (eventId: string, egId: string) => {
		const res = await service_api.post<UniResponse<AwdpInstance>>(
			`/events/${eventId}/awdp/gameboxes/${egId}/instance`,
		);
		return res.data;
	},
	stopInstance: async (eventId: string, egId: string) => {
		const res = await service_api.post<UniResponse<null>>(
			`/events/${eventId}/awdp/gameboxes/${egId}/instance/stop`,
		);
		return res.data;
	},
	resetInstance: async (eventId: string, egId: string) => {
		const res = await service_api.post<UniResponse<AwdpInstance>>(
			`/events/${eventId}/awdp/gameboxes/${egId}/instance/reset`,
		);
		return res.data;
	},
	getInstance: async (eventId: string, egId: string) => {
		const res = await service_api.get<UniResponse<AwdpInstance | null>>(
			`/events/${eventId}/awdp/gameboxes/${egId}/instance`,
		);
		return res.data;
	},
	submitBreak: async (eventId: string, egId: string, flag: string) => {
		const res = await service_api.post<UniResponse<BreakSubmitResponse>>(
			`/events/${eventId}/awdp/gameboxes/${egId}/break`,
			{
				flag,
			},
		);
		return res.data;
	},
	uploadPatch: async (eventId: string, egId: string, file: File) => {
		const form = new FormData();
		form.append("patch_file", file);
		const res = await service_api.post<UniResponse<PatchSubmitResponse>>(
			`/events/${eventId}/awdp/gameboxes/${egId}/patch`,
			form,
		);
		return res.data;
	},
	testCheck: async (eventId: string, egId: string) => {
		const res = await service_api.post<UniResponse<ManualCheckDto>>(
			`/events/${eventId}/awdp/gameboxes/${egId}/test-check`,
		);
		return res.data;
	},
	sourceUrl: async (eventId: string, egId: string) => {
		const res = await service_api.get<UniResponse<string>>(
			`/events/${eventId}/awdp/gameboxes/${egId}/source`,
		);
		return res.data;
	},
	rounds: async (eventId: string) => {
		const res = await service_api.get<UniResponse<AwdpRoundDto[]>>(
			`/events/${eventId}/awdp/rounds`,
		);
		return res.data;
	},
	evaluations: async (eventId: string) => {
		const res = await service_api.get<UniResponse<AwdpEvaluationDto[]>>(
			`/events/${eventId}/awdp/evaluations`,
		);
		return res.data;
	},
	scores: async (eventId: string) => {
		const res = await service_api.get<UniResponse<AwdpScoreRow[]>>(
			`/events/${eventId}/awdp/scores`,
		);
		return res.data;
	},
};

export type AwdpRoundDto = {
	id: string;
	sequence: number;
	starts_at: string;
	cutoff_at: string;
	status: string;
};

export type AwdpEvaluationDto = {
	id: string;
	instance_id: string;
	event_gamebox_id: string;
	fix_round_id: string | null;
	round_sequence: number | null;
	kind: "manual" | "official";
	status: string;
	healthcheck_result: string | null;
	judge_result: string | null;
	finished_at: string | null;
};
