/**
 * AWDP 管理端 + 选手端 API 客户端。
 * 管理端：/api/admin/events/{eventId}/awdp/...
 * 选手端：/api/events/{eventId}/awdp/...
 */
import { type UniResponse, admin_api, service_api } from "@/api/axios";

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
};

export type ManualCheckDto = {
	healthcheck_ok: boolean;
	healthcheck_detail: string[];
	judge_ok: boolean;
	judge_detail: string;
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
	expected_updated_at: string;
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

// ────────────────────────────────────────────────────────────────────────────
// Admin client
// ────────────────────────────────────────────────────────────────────────────

export const awdpAdminApi = {
	getConfig: (eventId: string) =>
		admin_api.get<UniResponse<AwdpEventConfigDto>>(`/events/${eventId}/awdp`),
	updateConfig: (eventId: string, body: AwdpConfigPatchInput) =>
		admin_api.patch<UniResponse<AwdpEventConfigDto>>(`/events/${eventId}/awdp`, body),
	start: (eventId: string) =>
		admin_api.post<UniResponse<null>>(`/events/${eventId}/awdp/start`),
	breakToFix: (eventId: string) =>
		admin_api.post<UniResponse<null>>(`/events/${eventId}/awdp/break-to-fix`),
	finish: (eventId: string) =>
		admin_api.post<UniResponse<null>>(`/events/${eventId}/awdp/finish`),
	attachGamebox: (eventId: string, gameboxId: string, hidden?: boolean) =>
		admin_api.post<UniResponse<AwdpAdminEventGameBoxDto>>(`/events/${eventId}/awdp/gameboxes`, {
			gamebox_id: gameboxId,
			hidden,
		}),
	detachGamebox: (eventId: string, egId: string) =>
		admin_api.delete<UniResponse<null>>(`/events/${eventId}/awdp/gameboxes/${egId}`),
	listEventGameboxes: (eventId: string) =>
		admin_api.get<UniResponse<AwdpAdminEventGameBoxDto[]>>(`/events/${eventId}/awdp/gameboxes`),
	listInstances: (eventId: string) =>
		admin_api.get<UniResponse<AwdpAdminInstanceDto[]>>(`/events/${eventId}/awdp/instances`),
};

// ────────────────────────────────────────────────────────────────────────────
// Player client
// ────────────────────────────────────────────────────────────────────────────

export const awdpPlayerApi = {
	overview: (eventId: string) =>
		service_api.get<UniResponse<AwdpOverview>>(`/events/${eventId}/awdp`),
	startInstance: (eventId: string, egId: string) =>
		service_api.post<UniResponse<AwdpInstance>>(`/events/${eventId}/awdp/gameboxes/${egId}/instance`),
	stopInstance: (eventId: string, egId: string) =>
		service_api.post<UniResponse<null>>(`/events/${eventId}/awdp/gameboxes/${egId}/instance/stop`),
	resetInstance: (eventId: string, egId: string) =>
		service_api.post<UniResponse<AwdpInstance>>(`/events/${eventId}/awdp/gameboxes/${egId}/instance/reset`),
	getInstance: (eventId: string, egId: string) =>
		service_api.get<UniResponse<AwdpInstance | null>>(`/events/${eventId}/awdp/gameboxes/${egId}/instance`),
	submitBreak: (eventId: string, egId: string, flag: string) =>
		service_api.post<UniResponse<BreakSubmitResponse>>(`/events/${eventId}/awdp/gameboxes/${egId}/break`, {
			flag,
		}),
	uploadPatch: (eventId: string, egId: string, file: File) => {
		const form = new FormData();
		form.append("patch_file", file);
		return service_api.post<UniResponse<PatchSubmitResponse>>(
			`/events/${eventId}/awdp/gameboxes/${egId}/patch`,
			form,
		);
	},
	testCheck: (eventId: string, egId: string) =>
		service_api.post<UniResponse<ManualCheckDto>>(`/events/${eventId}/awdp/gameboxes/${egId}/test-check`),
	sourceUrl: (eventId: string, egId: string) =>
		service_api.get<UniResponse<string>>(`/events/${eventId}/awdp/gameboxes/${egId}/source`),
	rounds: (eventId: string) =>
		service_api.get<UniResponse<AwdpRoundDto[]>>(`/events/${eventId}/awdp/rounds`),
	evaluations: (eventId: string) =>
		service_api.get<UniResponse<AwdpEvaluationDto[]>>(`/events/${eventId}/awdp/evaluations`),
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
