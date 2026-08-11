import {
	type AwdpEndpoint,
	type AwdpPhase,
	type AwdpRoundDto,
	type BreakSubmitResponse,
	type ManualCheckDto,
	type PatchSubmitResponse,
} from "@/api/awdp";
/**
 * AWDP Practice Run（Training Ground）API 客户端。
 *
 * 后端并行实施 run 中心化（awdp_runs 表 + `/api/service` scope），
 * 本文件按冻结契约编写（见 chore/plans/agent-contract-frontend.md §C.1/C.2）：
 *
 *   GET   /api/service/gameboxes?capability=awdp
 *   POST  /api/service/gameboxes/{gamebox_id}/awdp/runs
 *   GET   /api/service/awdp/runs/{run_id}
 *   POST  /api/service/awdp/runs/{run_id}/stop | /reset | /restart-training
 *   GET   /api/service/awdp/runs/{run_id}/rounds
 *   GET   /api/service/awdp/runs/{run_id}/evaluations
 *   GET   /api/service/awdp/runs/{run_id}/scores
 *   POST  /api/service/awdp/runs/{run_id}/gameboxes/{gamebox_id}/break
 *   POST  /api/service/awdp/runs/{run_id}/gameboxes/{gamebox_id}/patch
 *   POST  /api/service/awdp/runs/{run_id}/gameboxes/{gamebox_id}/test-check
 *   GET   /api/service/awdp/runs/{run_id}/gameboxes/{gamebox_id}/source
 *   POST  /api/service/awdp/runs/{run_id}/gameboxes/{gamebox_id}/instance
 *        | .../instance/stop | .../instance/reset | GET .../instance
 *
 * run 系 DTO 独立定义（不复用 event 系 DTO 以免字段漂移）；复用的仅有
 * 形状完全一致的 `AwdpRoundDto` / `BreakSubmitResponse` / `PatchSubmitResponse` /
 * `ManualCheckDto` / `AwdpPhase` / `AwdpEndpoint`。
 */
import { type QueryParams, type UniResponse, service_api } from "@/api/axios";

// ────────────────────────────────────────────────────────────────────────────
// Types
// ────────────────────────────────────────────────────────────────────────────

/** 安全目录条目（§56：不含 exploit/source 相关字段）。 */
export type GameBoxCatalogDto = {
	id: string;
	name: string;
	description: string;
	category: string;
	version: string | null;
	awdp_capable: boolean;
	recommended_cpu_millis: number;
	recommended_memory_bytes: number;
	recommended_pids_limit: number;
	/** 当前用户同 gamebox 的 active Practice Run（若有）。 */
	active_training: { run_id: string; phase: AwdpPhase } | null;
};

/** Run 内逻辑实例。 */
export type RunInstanceDto = {
	instance_id: string;
	gamebox_id: string;
	runtime_state: string;
	runtime_generation: number;
	broken: boolean;
	endpoints: AwdpEndpoint[];
};

/** AWDP Run 统一 view-model 数据源（Practice 与 Competition 共用引擎）。 */
export type AwdpRunDto = {
	run_id: string;
	gamebox_id: string;
	event_id: string | null;
	phase: AwdpPhase;
	break_duration_secs: number;
	fix_duration_secs: number;
	fix_round_interval_secs: number;
	break_score: number;
	fix_round_score: number;
	total_rounds: number;
	started_at: string | null;
	break_ends_at: string | null;
	fix_started_at: string | null;
	fix_ends_at: string | null;
	finished_at: string | null;
	current_round: number;
	next_action_at: string | null;
	my_score: number;
	/** Fix 阶段才非空。 */
	source_code_dir: string | null;
	instances: RunInstanceDto[];
};

/** run 化后的评估（列形状对齐 awdp_evaluations 的 run_id/gamebox_id 重构）。 */
export type AwdpRunEvaluationDto = {
	id: string;
	instance_id: string;
	gamebox_id: string;
	fix_round_id: string | null;
	round_sequence: number | null;
	kind: "manual" | "official";
	status: string;
	healthcheck_result: string | null;
	judge_result: string | null;
	finished_at: string | null;
};

/** 计分明细（append-only ledger）。 */
export type ScoreEventDto = {
	id: string;
	score_type: "break" | "fix";
	gamebox_id: string;
	fix_round_id: string | null;
	delta: number;
	created_at: string;
};

export type AwdpRunScoresDto = {
	total: number;
	history: ScoreEventDto[];
};

/** source presigned URL 响应（§C.2：GET .../source → UniResponse<{url:string}>）。 */
export type SourceUrlResponse = {
	url: string;
};

// ────────────────────────────────────────────────────────────────────────────
// Client
// ────────────────────────────────────────────────────────────────────────────

export const awdpRunApi = {
	// 目录 / Start Training（§56-57）
	gameboxCatalog: async (params?: QueryParams) => {
		const res = await service_api.get<UniResponse<GameBoxCatalogDto[]>>(
			"/service/gameboxes",
			{ params: { ...params, capability: "awdp" } },
		);
		return res.data;
	},
	startTraining: async (gameboxId: string) => {
		const res = await service_api.post<UniResponse<AwdpRunDto>>(
			`/service/gameboxes/${gameboxId}/awdp/runs`,
		);
		return res.data;
	},

	// Practice Run（§58）
	getRun: async (runId: string) => {
		const res = await service_api.get<UniResponse<AwdpRunDto>>(
			`/service/awdp/runs/${runId}`,
		);
		return res.data;
	},
	stopRun: async (runId: string) => {
		const res = await service_api.post<UniResponse<AwdpRunDto | null>>(
			`/service/awdp/runs/${runId}/stop`,
		);
		return res.data;
	},
	resetRun: async (runId: string) => {
		const res = await service_api.post<UniResponse<AwdpRunDto | null>>(
			`/service/awdp/runs/${runId}/reset`,
		);
		return res.data;
	},
	restartTraining: async (runId: string) => {
		const res = await service_api.post<UniResponse<AwdpRunDto | null>>(
			`/service/awdp/runs/${runId}/restart-training`,
		);
		return res.data;
	},
	rounds: async (runId: string) => {
		const res = await service_api.get<UniResponse<AwdpRoundDto[]>>(
			`/service/awdp/runs/${runId}/rounds`,
		);
		return res.data;
	},
	evaluations: async (runId: string) => {
		const res = await service_api.get<UniResponse<AwdpRunEvaluationDto[]>>(
			`/service/awdp/runs/${runId}/evaluations`,
		);
		return res.data;
	},
	scores: async (runId: string) => {
		const res = await service_api.get<UniResponse<AwdpRunScoresDto>>(
			`/service/awdp/runs/${runId}/scores`,
		);
		return res.data;
	},

	// run-scoped gamebox 子资源
	submitBreak: async (runId: string, gameboxId: string, flag: string) => {
		const res = await service_api.post<UniResponse<BreakSubmitResponse>>(
			`/service/awdp/runs/${runId}/gameboxes/${gameboxId}/break`,
			{ flag },
		);
		return res.data;
	},
	uploadPatch: async (runId: string, gameboxId: string, file: File) => {
		const form = new FormData();
		form.append("patch_file", file);
		const res = await service_api.post<UniResponse<PatchSubmitResponse>>(
			`/service/awdp/runs/${runId}/gameboxes/${gameboxId}/patch`,
			form,
		);
		return res.data;
	},
	testCheck: async (runId: string, gameboxId: string) => {
		const res = await service_api.post<UniResponse<ManualCheckDto>>(
			`/service/awdp/runs/${runId}/gameboxes/${gameboxId}/test-check`,
		);
		return res.data;
	},
	sourceUrl: async (runId: string, gameboxId: string) => {
		const res = await service_api.get<UniResponse<SourceUrlResponse>>(
			`/service/awdp/runs/${runId}/gameboxes/${gameboxId}/source`,
		);
		return res.data;
	},
	startInstance: async (runId: string, gameboxId: string) => {
		const res = await service_api.post<UniResponse<RunInstanceDto>>(
			`/service/awdp/runs/${runId}/gameboxes/${gameboxId}/instance`,
		);
		return res.data;
	},
	stopInstance: async (runId: string, gameboxId: string) => {
		const res = await service_api.post<UniResponse<RunInstanceDto | null>>(
			`/service/awdp/runs/${runId}/gameboxes/${gameboxId}/instance/stop`,
		);
		return res.data;
	},
	resetInstance: async (runId: string, gameboxId: string) => {
		const res = await service_api.post<UniResponse<RunInstanceDto>>(
			`/service/awdp/runs/${runId}/gameboxes/${gameboxId}/instance/reset`,
		);
		return res.data;
	},
	getInstance: async (runId: string, gameboxId: string) => {
		const res = await service_api.get<UniResponse<RunInstanceDto | null>>(
			`/service/awdp/runs/${runId}/gameboxes/${gameboxId}/instance`,
		);
		return res.data;
	},
};
