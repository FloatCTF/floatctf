/**
 * AWDP 选手端语义 —— 阶段 / 评测状态 / 动作可用性的**唯一**映射出处。
 *
 * 阶段与动作门禁全部来自后端真实约束（`apps/api/src/modules/event/awdp/`）：
 * - `break_service::submit_flag`：`phase != Break` → InvalidState（只有 Break 阶段能交 flag）；
 * - `patch_service::apply_patch`：`phase != Fix` → InvalidState；且实例必须 `running`；
 * - `api/player.rs::download_source`：`phase != Fix` → InvalidState（源码仅在 Fix 阶段）；
 * - `api/player.rs::manual_test_check`：实例未启动 → NotFound（Test Check 前必须先在跑实例）。
 * 因此前端把这几条做成**显式禁用 + 原因**，而不是让用户点了再吃 4xx。
 */

import type { AwdpInstance, AwdpPhase } from "@floatctf/sdk";

import type { PillTone } from "~/components/app/badges";
import { isFloatCTFError } from "~/api/errors";

export interface AwdpGate {
	allowed: boolean;
	/** 不允许时的原因（必须渲染出来，不静默禁用）。 */
	reason: string;
}

const ALLOWED: AwdpGate = { allowed: true, reason: "" };

/** `api/awdp.ts` 的 `AwdpPhase` 字符串联合 —— 含过渡态 `preparing_fix`。 */
export const AWDP_PHASE_ORDER: readonly AwdpPhase[] = [
	"pending",
	"break",
	"preparing_fix",
	"fix",
	"ended",
];

const PHASE_LABEL: Record<AwdpPhase, string> = {
	pending: "待开始",
	break: "Break 破题",
	preparing_fix: "准备 Fix（过渡）",
	fix: "Fix 修复",
	ended: "已结束",
};

const PHASE_TONE: Record<AwdpPhase, PillTone> = {
	pending: "muted",
	break: "info",
	preparing_fix: "warning",
	fix: "success",
	ended: "muted",
};

export function awdpPhaseLabel(phase: AwdpPhase | null | undefined): string {
	if (!phase) return "未知";
	return PHASE_LABEL[phase] ?? phase;
}

export function awdpPhaseTone(phase: AwdpPhase | null | undefined): PillTone {
	if (!phase) return "muted";
	return PHASE_TONE[phase] ?? "neutral";
}

export function awdpPhaseIndex(phase: AwdpPhase | null | undefined): number {
	if (!phase) return -1;
	return AWDP_PHASE_ORDER.indexOf(phase);
}

/** `preparing_fix` 的中文说明（过渡态，不可操作）。 */
export const AWDP_PREPARING_FIX_NOTE =
	"Break 阶段已结束，后端正在准备 Fix 环境（把实例重置为原始状态、物化回合时间线）；此过渡态下不可提交 flag 或补丁。";

/** 官方评测 / manual 评估状态（`awdp_evaluation_status`）。 */
const EVAL_STATUS_LABEL: Record<string, string> = {
	pending: "待评测",
	running: "评测中",
	no_patch: "未提交补丁（+0）",
	service_down: "服务不可用（+0）",
	functional_broken: "功能被破坏（+0）",
	vulnerable: "漏洞仍可利用（+0）",
	patched: "已修复（计分）",
	platform_error: "平台错误",
};

const EVAL_STATUS_TONE: Record<string, PillTone> = {
	pending: "info",
	running: "info",
	patched: "success",
	no_patch: "warning",
	service_down: "danger",
	functional_broken: "danger",
	vulnerable: "warning",
	platform_error: "danger",
};

export function awdpEvaluationLabel(status: string | null | undefined): string {
	if (!status) return "—";
	return EVAL_STATUS_LABEL[status] ?? status;
}

export function awdpEvaluationTone(status: string | null | undefined): PillTone {
	if (!status) return "muted";
	return EVAL_STATUS_TONE[status] ?? "neutral";
}

/** 实例运行态：后端只写 `running` / `stopped`（`awdp/service/runtime.rs`）。 */
export function awdpRuntimeLabel(state: string | null | undefined): string {
	switch (state) {
		case "running":
			return "运行中";
		case "stopped":
			return "已停止";
		case null:
		case undefined:
		case "":
			return "未启动";
		default:
			return state;
	}
}

export function awdpRuntimeTone(state: string | null | undefined): PillTone {
	switch (state) {
		case "running":
			return "success";
		case "stopped":
			return "muted";
		case null:
		case undefined:
		case "":
			return "muted";
		default:
			return "neutral";
	}
}

export function isInstanceRunning(instance: AwdpInstance | null | undefined): boolean {
	return instance?.runtime_state === "running";
}

/** Break flag 提交：仅 Break 阶段。 */
export function awdpBreakGate(phase: AwdpPhase): AwdpGate {
	if (phase === "break") return ALLOWED;
	if (phase === "preparing_fix") {
		return { allowed: false, reason: "Break 已结束，正在准备 Fix 环境，不能再提交 flag。" };
	}
	return { allowed: false, reason: `只有 Break 阶段可以提交 flag（当前：${awdpPhaseLabel(phase)}）。` };
}

/** 补丁上传：仅 Fix 阶段，且实例必须运行中。 */
export function awdpPatchGate(phase: AwdpPhase, instance: AwdpInstance | null): AwdpGate {
	if (phase !== "fix") {
		return { allowed: false, reason: `只有 Fix 阶段可以上传补丁（当前：${awdpPhaseLabel(phase)}）。` };
	}
	if (!isInstanceRunning(instance)) {
		return { allowed: false, reason: "实例未运行，补丁无法应用；请先启动实例。" };
	}
	return ALLOWED;
}

/**
 * 实例生命周期动作（启动 / 停止 / 重置）的阶段门。
 * 后端 `service/runtime.rs::start_instance` 明确要求 `phase ∈ {Break, Fix}`（第 119-126 行），
 * 其它阶段直接 InvalidState；pending / ended 连 active run 都没有。
 */
export function awdpInstanceControlGate(phase: AwdpPhase): AwdpGate {
	if (phase === "break" || phase === "fix") return ALLOWED;
	if (phase === "pending") return { allowed: false, reason: "比赛尚未开始：实例操作不可用。" };
	if (phase === "preparing_fix") {
		return { allowed: false, reason: "准备 Fix 过渡态：实例操作暂不可用。" };
	}
	return { allowed: false, reason: "比赛已结束：实例操作已关闭。" };
}

/** 手工 Test Check：后端要求实例已启动（否则 404 instance not started）。 */
export function awdpTestCheckGate(instance: AwdpInstance | null): AwdpGate {
	if (!isInstanceRunning(instance)) {
		return { allowed: false, reason: "Test Check 需要运行中的实例：请先启动实例。" };
	}
	return ALLOWED;
}

/** 源码下载：仅 Fix 阶段（后端在其它阶段直接拒绝）。 */
export function awdpSourceGate(phase: AwdpPhase): AwdpGate {
	if (phase === "fix") return ALLOWED;
	return {
		allowed: false,
		reason: `源码压缩包仅在 Fix 阶段可下载（当前：${awdpPhaseLabel(phase)}）。`,
	};
}

/** 破坏性操作：实例重置 / run 重置与结束的真实后果。 */
export const AWDP_INSTANCE_RESET_CONSEQUENCES = [
	"实例容器会被销毁并按原始镜像重建（pristine）：容器内的修改、已上传的补丁与运行中的进程全部丢失。",
	"已提交的 Break 得分与历史评分记录**不受影响**（账本 append-only）。",
	"重建期间服务端点会短暂不可用，正在进行的自检 / 评测可能失败。",
];

export const AWDP_RUN_RESET_CONSEQUENCES = [
	"run 内的全部实例会被重置为原始状态（容器重建、补丁丢失）。",
	"run 与端点保留，阶段与计分账本不变。",
	"需要重新上传补丁并重新自检。",
];

export const AWDP_RUN_END_CONSEQUENCES = [
	"run 内全部实例会被停止（容器不再运行）。",
	"训练结束：本次 run 的 Break 得分与历史保留（账本不删除）。",
	"如需从头再来请使用「再次训练」，它会基于同一 GameBox 新建一个从 0 计分的 run。",
];

/**
 * 赛事未启用 AWDP：后端 `api/player.rs::get_overview` 对非 AWDP 赛事返回
 * `AppError::Validation("not an AWDP event")`。这属于业务态，应显示空态而不是错误块。
 */
export function isAwdpNotConfigured(error: unknown): boolean {
	if (!isFloatCTFError(error)) return false;
	return (error.platformMessage ?? "").toLowerCase().includes("not an awdp event");
}
