/**
 * AWD 选手端语义 —— **只做「后端字段 → 人话」的映射与动作可用性判定**，
 * 不自造任何数字与状态。
 *
 * 判定口径与官方 Default 前端 `service/events/awd.$id/{index,gameboxes}.tsx` 的
 * `flagAllowed` / `resetAllowed` 完全一致（那是后端语义的参照），字段全部取自
 * `AwdPlayerStatus`（`status` / `phase` / `banned` / `final_settlement`）。
 * 注意：`AwdPlayerStatus` **没有**免费重置次数与罚分字段（那在管理端 `AwdEventStatus`），
 * 因此本前端**不显示任何免费次数/罚分数字**，只在确认框里描述后端语义。
 */

import type { PillTone } from "~/components/app/badges";
import { isFloatCTFError } from "~/api/errors";

export interface AwdGate {
	allowed: boolean;
	/** 不允许时的原因（永远可见，不静默禁用）。 */
	reason: string;
}

const ALLOWED: AwdGate = { allowed: true, reason: "" };

/** AWD 赛事状态（`awd_event_status`）。 */
const STATUS_LABEL: Record<string, string> = {
	draft: "草稿",
	configuring: "配置中",
	deploying: "部署中",
	deployed: "已部署",
	prechecking: "预检中",
	verified: "已验证",
	running: "进行中",
	paused: "已暂停",
	network_error: "网络异常",
	start_blocked: "启动受阻",
	finished: "已结束",
	archived: "已归档",
	deploy_failed: "部署失败",
	verification_failed: "验证失败",
};

const STATUS_TONE: Record<string, PillTone> = {
	running: "success",
	verified: "success",
	deployed: "info",
	deploying: "info",
	prechecking: "info",
	configuring: "info",
	paused: "warning",
	start_blocked: "warning",
	network_error: "danger",
	deploy_failed: "danger",
	verification_failed: "danger",
	finished: "muted",
	archived: "muted",
	draft: "muted",
};

/** AWD 阶段（`awd_phase`）：hardening / attack / pause。 */
const PHASE_LABEL: Record<string, string> = {
	hardening: "加固期",
	attack: "攻击期",
	pause: "暂停",
};

const PHASE_TONE: Record<string, PillTone> = {
	hardening: "info",
	attack: "success",
	pause: "warning",
};

/** GameBox 实例状态（`gamebox_status` 枚举全量取值）。 */
const GAMEBOX_STATUS_LABEL: Record<string, string> = {
	pending: "等待中",
	creating: "创建中",
	running: "运行中",
	ready: "就绪",
	resetting: "重置中",
	missing: "缺失",
	orphan: "孤儿容器",
	conflict: "资源冲突",
	start_failed: "启动失败",
	reset_failed: "重置失败",
	stopped: "已停止",
};

const GAMEBOX_STATUS_TONE: Record<string, PillTone> = {
	pending: "muted",
	creating: "info",
	running: "success",
	ready: "success",
	resetting: "warning",
	missing: "danger",
	orphan: "danger",
	conflict: "danger",
	start_failed: "danger",
	reset_failed: "danger",
	stopped: "danger",
};

export function awdStatusLabel(status: string | null | undefined): string {
	if (!status) return "未知";
	return STATUS_LABEL[status] ?? status;
}

export function awdStatusTone(status: string | null | undefined): PillTone {
	if (!status) return "muted";
	return STATUS_TONE[status] ?? "neutral";
}

export function awdPhaseLabel(phase: string | null | undefined): string {
	if (!phase) return "未知";
	return PHASE_LABEL[phase] ?? phase;
}

export function awdPhaseTone(phase: string | null | undefined): PillTone {
	if (!phase) return "muted";
	return PHASE_TONE[phase] ?? "neutral";
}

export function gameboxStatusLabel(status: string | null | undefined): string {
	if (!status) return "未知";
	return GAMEBOX_STATUS_LABEL[status] ?? status;
}

export function gameboxStatusTone(status: string | null | undefined): PillTone {
	if (!status) return "muted";
	return GAMEBOX_STATUS_TONE[status] ?? "neutral";
}

/** 由 `AwdPlayerStatus` 得到的「比赛已关闭」原因；null = 未关闭。 */
function closedReason(status: AwdStatusFields | null): string | null {
	if (!status) return "AWD 尚未配置。";
	if (status.banned) return "你的队伍已被禁赛，所有选手端操作已关闭。";
	if (status.status === "finished" || status.status === "archived") return "比赛已结束。";
	if (status.final_settlement) return "最终结算中 —— 比赛已关闭，等待 Judge 结算完成。";
	if (status.status === "paused" || status.phase === "pause") return "比赛已暂停。";
	if (status.status === "network_error") return "基础设施网络异常，暂不可操作。";
	return null;
}

export interface AwdStatusFields {
	status: string;
	phase: string;
	banned: boolean;
	final_settlement: boolean;
}

/** flag 提交（攻击得分）是否可用 —— 仅在攻击期、且比赛未关闭时。 */
export function awdFlagGate(status: AwdStatusFields | null): AwdGate {
	const closed = closedReason(status);
	if (closed) return { allowed: false, reason: closed };
	if (status?.phase === "hardening") return { allowed: false, reason: "攻击尚未开始（加固期）。" };
	if (status?.phase !== "attack") return { allowed: false, reason: "当前阶段不可提交 flag。" };
	return ALLOWED;
}

/** GameBox 重置是否可用 —— 加固期与攻击期可用；暂停/结算/封禁/网络异常均不可用。 */
export function awdResetGate(status: AwdStatusFields | null): AwdGate {
	const closed = closedReason(status);
	if (closed) return { allowed: false, reason: closed };
	if (status?.phase === "hardening" || status?.phase === "attack") return ALLOWED;
	return { allowed: false, reason: "当前阶段不可重置 GameBox。" };
}

/** GameBox 重置的真实后果（后端语义；免费次数与罚分由赛事配置决定，本前端不臆造数字）。 */
export const AWD_RESET_CONSEQUENCES = [
	"GameBox 容器会被销毁，并按原始镜像重新构建：你在容器内的所有修改都会丢失。",
	"重置受赛事配置的免费重置次数与超次数罚分约束 —— 免费次数用尽后继续重置会按赛事规则扣分。",
	"重置期间该 GameBox 会短暂不可用，SSH / WireGuard 连接可能中断，需要重新连接。",
];

export interface AwdUnavailableHint {
	title: string;
	description: string;
}

/**
 * 把「网络凭据尚未可用」从**加载失败**里区分出来 —— 返回 null 表示这是真正的失败
 * （网络错误 / 5xx / 未知业务错误），调用方应该渲染 `ErrorBlock`。
 *
 * 判定证据（后端源码 `apps/api/src/modules/event/awd/`，实测响应与之一致）：
 * - `repo/event_network_repo.rs::require_by_event_id` → `AwdError::NotFound("event network for {id}")`
 *   → HTTP 404，`platformMessage` 形如 `event network for <uuid>`（赛事尚未分配网络）；
 * - `wireguard/config` 里 `awd_team_networks` 缺失 → `NotFound("Team network not configured")`（队伍网络尚未分配）；
 * - `wireguard/config` / `ssh-config` 未加入战队 → `NotFound("你尚未加入队伍")`；
 * - `wireguard/config` 私钥二次拉取 → `Forbidden("WireGuard 私钥仅首次拉取返回…")`；
 * - 赛事终态 / 最终结算 → `Forbidden("赛事已结束…" / "正在最终结算…")`。
 * 因此：404 一律视为「业务上尚不可用」；403 视为「访问已关闭 / 私钥已下发」；
 * 其余（含 5xx、网络错误）仍走错误块。任何情况下都保留后端原文，不隐藏信息。
 */
export function awdCredentialsUnavailable(error: unknown): AwdUnavailableHint | null {
	if (!isFloatCTFError(error)) return null;
	const message = (error.platformMessage ?? "").trim();
	const lower = message.toLowerCase();

	if (lower.includes("event network")) {
		return {
			title: "赛事网络尚未分配",
			description:
				"管理员尚未为这场赛事分配 GameBox / WireGuard 网段，也未完成部署；分配并部署后这里会出现你的 WireGuard 配置与 SSH 凭据。",
		};
	}
	if (lower.includes("team network")) {
		return {
			title: "队伍网络尚未分配",
			description:
				"赛事网络已分配，但你的队伍还没有网络段（通常发生在队伍晚于部署创建）；需要管理员重新部署。",
		};
	}
	if (message.includes("加入队伍")) {
		return {
			title: "尚未加入战队",
			description: "加入本赛事的战队后，即可领取 WireGuard 配置与 SSH 凭据。",
		};
	}
	if (message.includes("私钥")) {
		return {
			title: "私钥已下发过",
			description:
				"WireGuard 私钥只在首次拉取时返回；如需重新获取，请联系管理员轮换密钥。",
		};
	}
	if (error.httpStatus === 404) {
		return {
			title: "网络凭据尚未可用",
			description: message
				? `${message} —— 赛事尚未部署完成，或你的队伍还没有可用实例；管理员部署后这里会出现凭据。`
				: "赛事尚未部署完成，或你的队伍还没有可用实例；管理员部署后这里会出现凭据。",
		};
	}
	if (error.httpStatus === 403) {
		return {
			title: "网络访问已关闭",
			description: message || "赛事已结束或正在最终结算，网络访问已关闭。",
		};
	}
	return null;
}
