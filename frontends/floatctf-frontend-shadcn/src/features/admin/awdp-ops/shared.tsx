/**
 * AWDP 运维页的共用语义层：阶段文案、生命周期可用性、破坏性操作后果、结局判定。
 *
 * AWDP 阶段状态机（后端 `AwdpPhaseExt::can_transition_to`）是**线性**的：
 * `pending → break → preparing_fix → fix → ended`，禁止跳级与回退；
 * `preparing_fix` 是真实的过渡态（实例正在被重置为 pristine），必须如实展示。
 */

import type { ReactNode } from "react";

import type { FloatCTFClient } from "@floatctf/sdk";

import { callVoid } from "~/api/call";
import { isFloatCTFError } from "~/api/errors";
import { TonePill, type PillTone } from "~/components/app/badges";
import type { ConfirmOptions } from "~/components/app/confirm";

export const AWDP_PHASE_LABEL: Record<string, string> = {
	pending: "未开始",
	break: "攻破阶段",
	preparing_fix: "准备修复（过渡态）",
	fix: "修复阶段",
	ended: "已结束",
};

export const AWDP_PHASE_TONE: Record<string, PillTone> = {
	pending: "muted",
	break: "info",
	preparing_fix: "warning",
	fix: "success",
	ended: "neutral",
};

export const AWDP_PHASE_DESCRIPTION: Record<string, string> = {
	pending: "尚未开始：等待手动开始或到点自动开始。",
	break: "攻破阶段：选手提交 flag 得分，倒计时结束进入修复阶段。",
	preparing_fix: "过渡态：平台正在把全部实例重置为 pristine，完成后自动进入修复阶段。",
	fix: "修复阶段：按回合 cutoff 自动评估并计分。",
	ended: "已结束：不再接受 flag / 修正包提交，遗留评估仍会结算。",
};

export function awdpPhaseLabel(phase: string | null | undefined): string {
	if (!phase) return "未知";
	return AWDP_PHASE_LABEL[phase] ?? phase;
}

export function awdpPhaseTone(phase: string | null | undefined): PillTone {
	if (!phase) return "muted";
	return AWDP_PHASE_TONE[phase] ?? "neutral";
}

export function AwdpPhasePill({ phase }: { phase: string | null | undefined }): ReactNode {
	return (
		<TonePill tone={awdpPhaseTone(phase)} title={AWDP_PHASE_DESCRIPTION[phase ?? ""]}>
			{awdpPhaseLabel(phase)}
		</TonePill>
	);
}

/* ── 生命周期动作 ─────────────────────────────────────────────────────────── */

export type AwdpAction = "start" | "breakToFix" | "finish";

export const AWDP_ACTION_LABEL: Record<AwdpAction, string> = {
	start: "开始比赛",
	breakToFix: "攻破 → 修复",
	finish: "结束比赛",
};

export async function runAwdpAction(
	client: FloatCTFClient,
	eventId: string,
	action: AwdpAction,
): Promise<void> {
	const admin = client.awdp.admin;
	switch (action) {
		case "start":
			return callVoid(admin.start(eventId), "开始比赛");
		case "breakToFix":
			return callVoid(admin.breakToFix(eventId), "进入修复阶段");
		case "finish":
			return callVoid(admin.finish(eventId), "结束比赛");
	}
}

export const AWDP_ACTION_CONFIRM: Record<AwdpAction, ConfirmOptions> = {
	start: {
		title: "开始 AWDP 比赛？",
		description: "创建本轮比赛 run 并立即进入攻破（Break）阶段。",
		consequences: [
			"以当前配置快照创建比赛 run（Break 时长、Fix 时长与分值）",
			"立即进入 Break 并开始倒计时；Break 到期后自动进入准备修复 → 修复阶段",
			"为所有已挂载且未隐藏的 GameBox 自动启动全部参与者的实例（幂等）",
			"开赛后配置锁定；本赛事结束后不能重新启动",
		],
		tone: "danger",
		confirmText: "开始比赛",
	},
	breakToFix: {
		title: "推进到修复阶段？（会重置全部实例）",
		description: "Break → 准备修复 → 修复；该动作会把所有实例恢复为初始状态。",
		consequences: [
			"把全部已启动实例重置为 pristine：runtime_generation +1，公开端口不变",
			"重置期间赛事处于 preparing_fix 过渡态；全部成功后才进入修复阶段",
			"任一实例重置失败会停留在 preparing_fix，可用同一按钮重试（重置幂等）",
			"进入修复阶段后按回合 cutoff 自动评估并计分",
		],
		tone: "danger",
		confirmText: "进入修复阶段",
	},
	finish: {
		title: "结束 AWDP 比赛？",
		description: "Fix → Ended：比赛立即结束。",
		consequences: [
			"赛事进入 ended：选手不能再提交 flag 或上传修正包",
			"已在队列中的官方评估仍会继续结算并计分",
			"会尽力停止本 run 的全部实例并清理赛事网络（best-effort；已提交的结束状态不会回滚）",
		],
		tone: "danger",
		confirmText: "结束比赛",
	},
};

export interface AwdpActionGate {
	enabled: boolean;
	reason: string | null;
}

/**
 * 阶段 → 动作可用性。与后端守卫一一对应：
 * - start：`create_competition_run` 要求无 active run（pending）；已有 ended run 时拒绝重启；
 * - breakToFix：`transition_break_to_fix` 从 break 推进，也允许从 preparing_fix 重试；
 * - finish：`finish_awdp_event` 只做 Fix → Ended。
 */
export function buildAwdpGates(phase: string | null): Record<AwdpAction, AwdpActionGate> {
	switch (phase) {
		case "pending":
			return {
				start: { enabled: true, reason: null },
				breakToFix: {
					enabled: false,
					reason: "尚未开始：需先开始比赛进入攻破阶段",
				},
				finish: { enabled: false, reason: "尚未开始：无法直接结束，需先开始比赛" },
			};
		case "break":
			return {
				start: { enabled: false, reason: "比赛已开始：重复开始是幂等的，但不会推进阶段" },
				breakToFix: { enabled: true, reason: null },
				finish: {
					enabled: false,
					reason: "后端只接受 Fix → Ended：请先推进到修复阶段再结束",
				},
			};
		case "preparing_fix":
			return {
				start: { enabled: false, reason: "比赛已开始" },
				breakToFix: {
					enabled: true,
					reason: null,
				},
				finish: {
					enabled: false,
					reason: "过渡态：需等待（或重试）实例重置完成并进入修复阶段后才能结束",
				},
			};
		case "fix":
			return {
				start: { enabled: false, reason: "比赛已开始" },
				breakToFix: { enabled: false, reason: "已处于修复阶段，无法回到攻破阶段" },
				finish: { enabled: true, reason: null },
			};
		case "ended":
			return {
				start: {
					enabled: false,
					reason: "赛事已结束：后端拒绝重新启动（已结束的赛事不能重开）",
				},
				breakToFix: { enabled: false, reason: "赛事已结束" },
				finish: { enabled: false, reason: "赛事已结束（终态）" },
			};
		default:
			return {
				start: { enabled: false, reason: "阶段未知：请先刷新赛事配置" },
				breakToFix: { enabled: false, reason: "阶段未知：请先刷新赛事配置" },
				finish: { enabled: false, reason: "阶段未知：请先刷新赛事配置" },
			};
	}
}

/** 乐观锁冲突（后端 409）：配置已被他人修改。 */
export function isAwdpConflict(error: unknown): boolean {
	return isFloatCTFError(error) && error.httpStatus === 409;
}

/** 后端 `AwdpConfig::validate()` 的回合间隔 / 时长约束（客户端先校验，后端仍是权威）。 */
export function validateFixTiming(fixDuration: number, interval: number): string | null {
	if (!Number.isInteger(fixDuration) || fixDuration <= 0) return "Fix 时长必须为正整数秒";
	if (!Number.isInteger(interval) || interval <= 0) return "回合间隔必须为正整数秒";
	if (fixDuration % interval !== 0) {
		return `Fix 时长需能被回合间隔整除（当前 ${fixDuration} / ${interval}）`;
	}
	return null;
}
