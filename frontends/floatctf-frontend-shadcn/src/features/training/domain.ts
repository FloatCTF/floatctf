/**
 * AWDP Training Ground（练习 Run）语义 —— 阶段 / 评测状态 / 生命周期门禁。
 *
 * 与 `features/awdp/domain.ts` 刻意各持一份（两个域互相独立、可单独审阅），
 * 值都来自同一份后端枚举与守卫：
 * - `practice_service::train_again`：仅 `ended` run 可「再次训练」（新建 run）；
 * - `api/training.rs::end_run`：仅 `break` / `fix` 可结束（pending / ended 拒绝）；
 * - `set_phase`：仅 practice，且只支持 break↔fix；
 * - `break_service::submit_flag`：仅 Break 阶段；`patch_service`：仅 Fix 阶段 + 实例 running；
 * - `download_source`：仅 Fix 阶段；`manual_test_check`：需要运行中的实例。
 */

import type { AwdpInstance, AwdpPhase } from "@floatctf/sdk";

import type { PillTone } from "~/components/app/badges";

export interface TrainingGate {
	allowed: boolean;
	reason: string;
}

const ALLOWED: TrainingGate = { allowed: true, reason: "" };

export const RUN_PHASE_ORDER: readonly AwdpPhase[] = [
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

export function runPhaseLabel(phase: AwdpPhase | null | undefined): string {
	if (!phase) return "未知";
	return PHASE_LABEL[phase] ?? phase;
}

export function runPhaseTone(phase: AwdpPhase | null | undefined): PillTone {
	if (!phase) return "muted";
	return PHASE_TONE[phase] ?? "neutral";
}

export function runPhaseIndex(phase: AwdpPhase | null | undefined): number {
	if (!phase) return -1;
	return RUN_PHASE_ORDER.indexOf(phase);
}

export const RUN_PREPARING_FIX_NOTE =
	"Break 已结束，后端正在准备 Fix 环境（实例重置为原始状态、物化回合时间线）；过渡态下不可提交 flag 或补丁。";

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

export function runEvaluationLabel(status: string | null | undefined): string {
	if (!status) return "—";
	return EVAL_STATUS_LABEL[status] ?? status;
}

export function runEvaluationTone(status: string | null | undefined): PillTone {
	if (!status) return "muted";
	return EVAL_STATUS_TONE[status] ?? "neutral";
}

export function runRuntimeLabel(state: string | null | undefined): string {
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

export function runRuntimeTone(state: string | null | undefined): PillTone {
	switch (state) {
		case "running":
			return "success";
		case "stopped":
			return "muted";
		default:
			return "muted";
	}
}

export function isRunInstanceRunning(instance: AwdpInstance | null | undefined): boolean {
	return instance?.runtime_state === "running";
}

/** 「开始训练（Launch）」：pending / break / fix 可启动或继续；ended 终态拒绝。 */
export function startRunGate(phase: AwdpPhase): TrainingGate {
	if (phase === "ended") {
		return { allowed: false, reason: "run 已结束：请使用「再次训练」新建 run。" };
	}
	return ALLOWED;
}

/** 「结束训练」：仅 Break / Fix 阶段（pending / ended 后端拒绝）。 */
export function endRunGate(phase: AwdpPhase): TrainingGate {
	if (phase === "break" || phase === "fix") return ALLOWED;
	return {
		allowed: false,
		reason: `只有 Break / Fix 阶段的 run 可以结束（当前：${runPhaseLabel(phase)}）。`,
	};
}

/** 「再次训练」：仅 ended run（后端新建一个从 0 计分的 run）。 */
export function restartTrainingGate(phase: AwdpPhase): TrainingGate {
	if (phase === "ended") return ALLOWED;
	return {
		allowed: false,
		reason: `只有已结束的 run 可以再次训练（当前：${runPhaseLabel(phase)}）；如需重来请先结束训练。`,
	};
}

/** 手动切阶段：仅 break ↔ fix。 */
export function setPhaseGate(phase: AwdpPhase, target: "break" | "fix"): TrainingGate {
	if (phase === "break" && target === "fix") return ALLOWED;
	if (phase === "fix" && target === "break") return ALLOWED;
	return {
		allowed: false,
		reason: `当前阶段（${runPhaseLabel(phase)}）不能切换到 ${target === "fix" ? "Fix" : "Break"}。`,
	};
}

export function breakRunGate(phase: AwdpPhase): TrainingGate {
	if (phase === "break") return ALLOWED;
	return { allowed: false, reason: `只有 Break 阶段可以提交 flag（当前：${runPhaseLabel(phase)}）。` };
}

export function patchRunGate(phase: AwdpPhase, instance: AwdpInstance | null): TrainingGate {
	if (phase !== "fix") {
		return { allowed: false, reason: `只有 Fix 阶段可以上传补丁（当前：${runPhaseLabel(phase)}）。` };
	}
	if (!isRunInstanceRunning(instance)) {
		return { allowed: false, reason: "实例未运行，补丁无法应用；请先启动实例。" };
	}
	return ALLOWED;
}

/**
 * 实例生命周期动作（启动 / 停止 / 重置）的阶段门。
 * 后端 `service/runtime.rs::start_instance` 要求 `phase ∈ {Break, Fix}`（练习与比赛同一实现）：
 * pending 阶段要先用「开始训练」Launch，ended 阶段要「再次训练」。
 */
export function runInstanceControlGate(phase: AwdpPhase): TrainingGate {
	if (phase === "break" || phase === "fix") return ALLOWED;
	if (phase === "pending") {
		return { allowed: false, reason: "run 尚未开始：请先点「开始训练」（Launch）。" };
	}
	if (phase === "preparing_fix") {
		return { allowed: false, reason: "准备 Fix 过渡态：实例操作暂不可用。" };
	}
	return { allowed: false, reason: "run 已结束：请使用「再次训练」新建 run。" };
}

export function testCheckRunGate(instance: AwdpInstance | null): TrainingGate {
	if (!isRunInstanceRunning(instance)) {
		return { allowed: false, reason: "Test Check 需要运行中的实例：请先启动实例。" };
	}
	return ALLOWED;
}

/** ALL Check：需要运行中的实例；成功即结束本 run。 */
export function allCheckRunGate(phase: AwdpPhase, instance: AwdpInstance | null): TrainingGate {
	if (phase !== "fix") {
		return { allowed: false, reason: `ALL Check 只在 Fix 阶段可用（当前：${runPhaseLabel(phase)}）。` };
	}
	if (!isRunInstanceRunning(instance)) {
		return { allowed: false, reason: "ALL Check 需要运行中的实例：请先启动实例。" };
	}
	return ALLOWED;
}

export function sourceRunGate(phase: AwdpPhase): TrainingGate {
	if (phase === "fix") return ALLOWED;
	return { allowed: false, reason: `源码压缩包仅在 Fix 阶段可下载（当前：${runPhaseLabel(phase)}）。` };
}

export const RUN_STOP_CONSEQUENCES = [
	"run 内全部实例容器会被停止，暴露的服务端点不再可用。",
	"run 本身、端点、Break 得分与历史都保留；可以在「开始训练」里继续会话。",
	"停止期间官方回合判定无法执行（需要运行中的实例）。",
];

export const RUN_RESET_CONSEQUENCES = [
	"run 内全部实例会被重置为原始状态（容器销毁重建，补丁与容器内修改全部丢失）。",
	"run、端点与计分账本不变；需要重新上传补丁并重新自检。",
	"重建期间服务短暂不可用，正在进行的自检 / 评测会失败。",
];

export const RUN_END_CONSEQUENCES = [
	"run 内全部实例会被停止，训练会话结束。",
	"本次 run 的 Break 得分与评测历史**保留**（End 不删除账本）。",
	"结束后只能「再次训练」：基于同一 GameBox 新建一个从 0 计分的 run。",
];

export const RUN_INSTANCE_RESET_CONSEQUENCES = [
	"实例容器会被销毁并按原始镜像重建（pristine）：容器内修改与已上传补丁全部丢失。",
	"已入账的得分与评测历史不受影响。",
	"重建期间服务端点短暂不可用。",
];

export const RUN_RESTART_CONSEQUENCES = [
	"当前 run 已结束，会新建一个 run（新的 run 页面地址）。",
	"新 run 从 0 计分；旧 run 的得分与历史仍然保留，可在积分明细里查看。",
];

export const RUN_SET_PHASE_FIX_CONSEQUENCES = [
	"立即进入 Fix 阶段：实例会被重置为原始状态（pristine），Break 阶段的容器内修改丢失。",
	"后端开始物化 Fix 回合时间线，之后按回合做官方判定。",
];

export const RUN_SET_PHASE_BREAK_CONSEQUENCES = [
	"回到 Break 阶段会**撤销整个 Fix 会话**：已物化的回合、手动/官方评估与 Fix 计分全部清零，并重新物化全新时间线。",
	"Break 阶段已入账的得分不受影响。",
	"需要重新进入 Fix 阶段并重新提交补丁。",
];

export const RUN_ALL_CHECK_CONSEQUENCES = [
	"ALL Check 会立即用官方流程判定当前补丁（healthcheck → judge → exploit），不做二次确认。",
	"一旦通过（patched）：剩余回合全部按 Fix 计分，实例被停止，本 run 直接结束。",
	"未通过则不落账、不扣分，等本轮 cutoff 的官方 check 照常判定。",
];
