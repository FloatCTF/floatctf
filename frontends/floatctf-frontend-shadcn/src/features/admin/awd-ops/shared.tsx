/**
 * AWD 运维页的共用语义层：状态文案、生命周期可用性判定、破坏性操作的真实后果、预检报告解析。
 *
 * 两条硬约束（来自 CONVENTIONS §0.9 与 §2）：
 * - 状态判定**只**用后端返回的 `status` / `phase` / `final_settlement` / 网络分配结果，
 *   不在前端用时间或本地推导替代后端状态；
 * - 每个动作的禁用原因都写成可直接展示的文案（`title` + 页面提示），不出现「按钮灰着但不知为何」。
 */

import type { ReactNode } from "react";

import type { FloatCTFClient } from "@floatctf/sdk";

import { callVoid } from "~/api/call";
import { isFloatCTFError } from "~/api/errors";
import { TonePill, type PillTone } from "~/components/app/badges";
import type { ConfirmOptions } from "~/components/app/confirm";

/* ── 状态文案（后端 awd_event_status / awd_phase 的字面值）────────────────── */

export const AWD_STATUS_LABEL: Record<string, string> = {
	draft: "草稿",
	configuring: "配置中",
	deploying: "部署中",
	deployed: "已部署",
	prechecking: "预检中",
	verified: "预检通过",
	running: "进行中",
	paused: "已暂停",
	network_error: "网络异常",
	start_blocked: "开赛受阻",
	finished: "已结束",
	archived: "已归档",
	deploy_failed: "部署失败",
	verification_failed: "预检失败",
};

export const AWD_STATUS_TONE: Record<string, PillTone> = {
	draft: "muted",
	configuring: "info",
	deploying: "info",
	deployed: "info",
	prechecking: "info",
	verified: "success",
	running: "success",
	paused: "warning",
	network_error: "danger",
	start_blocked: "warning",
	finished: "neutral",
	archived: "muted",
	deploy_failed: "danger",
	verification_failed: "danger",
};

export const AWD_PHASE_LABEL: Record<string, string> = {
	hardening: "加固期",
	attack: "攻击期",
	pause: "已暂停",
};

export function awdStatusLabel(status: string | null | undefined): string {
	if (!status) return "未开通";
	return AWD_STATUS_LABEL[status] ?? status;
}

export function awdStatusTone(status: string | null | undefined): PillTone {
	if (!status) return "muted";
	return AWD_STATUS_TONE[status] ?? "neutral";
}

/** 状态 + 阶段徽章；阶段缺失时如实显示「—」。 */
export function AwdStatusPill({
	status,
	phase,
	finalSettlement = false,
}: {
	status: string | null | undefined;
	phase?: string | null;
	finalSettlement?: boolean;
}): ReactNode {
	return (
		<span className="inline-flex flex-wrap items-center gap-1.5">
			<TonePill tone={awdStatusTone(status)}>{awdStatusLabel(status)}</TonePill>
			{phase ? (
				<TonePill tone={phase === "pause" ? "warning" : "neutral"}>
					{AWD_PHASE_LABEL[phase] ?? phase}
				</TonePill>
			) : null}
			{finalSettlement ? (
				<TonePill tone="warning" title="最后一轮已完成、判题仍在结算：竞赛操作已关闭">
					终局结算中
				</TonePill>
			) : null}
		</span>
	);
}

/* ── 生命周期动作 ─────────────────────────────────────────────────────────── */

export type AwdAction =
	| "deploy"
	| "precheck"
	| "start"
	| "pause"
	| "resume"
	| "finish"
	| "archive"
	| "rotateTokens";

export const AWD_ACTION_LABEL: Record<AwdAction, string> = {
	deploy: "部署",
	precheck: "预检",
	start: "开赛",
	pause: "暂停",
	resume: "恢复",
	finish: "结束比赛",
	archive: "归档",
	rotateTokens: "轮换内部令牌",
};

/** 调用后端生命周期接口（返回 null 的动作统一走 `callVoid`）。 */
export async function runAwdAction(
	client: FloatCTFClient,
	eventId: string,
	action: AwdAction,
): Promise<void> {
	const awd = client.awd.admin;
	switch (action) {
		case "deploy":
			return callVoid(awd.deploy(eventId), "部署");
		case "precheck":
			return callVoid(awd.precheck(eventId), "预检");
		case "start":
			return callVoid(awd.start(eventId), "开赛");
		case "pause":
			return callVoid(awd.pause(eventId), "暂停");
		case "resume":
			return callVoid(awd.resume(eventId), "恢复");
		case "finish":
			return callVoid(awd.finish(eventId), "结束比赛");
		case "archive":
			return callVoid(awd.archive(eventId), "归档");
		case "rotateTokens":
			return callVoid(awd.rotateTokens(eventId), "轮换内部令牌");
	}
}

/**
 * 破坏性 / 不可逆操作的真实后果（逐条对应后端实现，不编造）。
 */
export const AWD_ACTION_CONFIRM: Record<AwdAction, ConfirmOptions> = {
	deploy: {
		title: "部署 AWD 运行时？",
		description: "按当前配置编排赛事运行时（幂等，可重复执行）。",
		consequences: [
			"创建赛事 Docker 网络、FlagServer / JudgeServer 容器与各战队 GameBox 实例容器",
			"为各战队分配网段并写入 WireGuard 对端，同时锁定赛事网络地址（锁定后不可再改分配）",
			"应用加固期防火墙策略；失败会进入 deploy_failed，可修复后重试",
		],
		tone: "danger",
		confirmText: "部署",
	},
	precheck: {
		title: "运行 AWD 预检？",
		description: "对配置 / 容器 / WireGuard / 网络 / flag / 判题链路做一次结构性与连通性检查。",
		consequences: [
			"预检期间赛事进入 prechecking，选手端操作不可用",
			"通过则状态变为 verified 并记录配置代数；失败则为 verification_failed 并落库失败原因",
		],
		confirmText: "开始预检",
	},
	start: {
		title: "开始 AWD 比赛？",
		description: "仅在预检通过（verified）且配置代数未被修改时可执行。",
		consequences: [
			"状态变为 running：加固时长为 0 时立即开始第 1 轮，否则先进入加固期",
			"为所有参赛战队写入初始分数账本（幂等，重复开赛不会重复加分）",
			"切换为攻防期防火墙策略；开赛后 AWD 运行时配置锁定",
		],
		tone: "danger",
		confirmText: "开赛",
	},
	pause: {
		title: "暂停比赛？",
		description: "冻结当前阶段并清理该赛事的活跃连接。",
		consequences: [
			"保存加固期 / 当前轮次的剩余时间，取消已排期的定时任务",
			"清理该赛事网段的连接（conntrack），选手端攻防操作中止",
			"恢复（resume）后从剩余时间继续，不会重开轮次",
		],
		confirmText: "暂停",
	},
	resume: {
		title: "恢复比赛？",
		description: "从 paused / network_error 回到进行中。",
		consequences: [
			"还原暂停前的阶段与剩余时间（加固剩余为 0 时直接进入攻击期）",
			"恢复活跃轮次与防火墙策略",
		],
		confirmText: "恢复",
	},
	finish: {
		title: "结束比赛？",
		description: "结束只在「终局结算」成立时生效；否则接口成功但赛事保持进行中。",
		consequences: [
			"仅在最后一轮已完成且全部判题任务处于终态时，状态才会变为 finished（积分定稿）",
			"未完成结算时不会被强制结束，也不会中断仍在跑的判题任务",
		],
		tone: "danger",
		confirmText: "结束比赛",
	},
	archive: {
		title: "归档赛事？（不可逆）",
		description: "归档会销毁运行时并锁定赛事。若赛事尚未结束，会先尝试走一次结束流程。",
		consequences: [
			"停止并删除该赛事全部容器：GameBox 实例、FlagServer、JudgeServer",
			"释放宿主 WireGuard 对端、防火墙策略与网段分配；赛事网络行保留为历史",
			"赛事状态变为 archived（终态），之后无法再修改、部署或重开",
			"数据库中的历史记录与积分保留；归档不可逆",
		],
		tone: "danger",
		confirmText: "归档",
		confirmPhrase: "archive",
	},
	rotateTokens: {
		title: "轮换内部令牌？（高危）",
		description: "key_version +1，重新加密并替换 FlagServer / JudgeServer 的内部调用令牌。",
		consequences: [
			"会原地重建 FlagServer / JudgeServer 容器（同网络 / 固定 IP）以加载新令牌",
			"重建期间判题与 flag 结算短暂不可用",
			"不改变题目 flag 本身（赛事密钥未变），选手已提交的 flag 不受影响",
			"写入内部令牌轮换审计记录",
		],
		tone: "danger",
		confirmText: "轮换",
		confirmPhrase: "rotate",
	},
};

export interface AwdActionGate {
	enabled: boolean;
	/** 不可用时的原因（直接展示给管理员）。 */
	reason: string | null;
}

export interface AwdGateInput {
	status: string | null;
	/** 赛事网络是否已分配（`getEventNetwork` 未返回 404）。 */
	networkAllocated: boolean;
	finalSettlement: boolean;
}

/** 后端 `AwdEventStatus::is_configurable()` 的字面值集合。 */
const CONFIGURABLE_STATUSES = [
	"draft",
	"configuring",
	"deployed",
	"prechecking",
	"verified",
	"start_blocked",
	"deploy_failed",
	"verification_failed",
];

/** 可（重新）部署的状态：处于运行前的配置态，允许修复后重试。 */
const DEPLOYABLE_STATUSES = [
	"draft",
	"configuring",
	"deployed",
	"deploy_failed",
	"verification_failed",
];

const TERMINAL_STATUSES = ["finished", "archived"];

function statusText(status: string | null): string {
	return status ? `当前状态为「${awdStatusLabel(status)}」` : "尚未开通 AWD";
}

/** 不可部署时的**逐状态**原因（比一句笼统的「当前状态不支持」更有用）。 */
const DEPLOY_BLOCK_REASON: Record<string, string> = {
	deploying: "部署正在进行中（deploying），请等待本次部署结束",
	prechecking: "预检进行中（prechecking），请等待预检结束",
	verified: "已通过预检：运行时已就绪，无需重新部署；修改配置后可再次部署",
	start_blocked: "开赛受阻（start_blocked）：请先重新预检通过",
	running: "比赛进行中：不能重新部署",
	paused: "比赛已暂停：请先恢复或结束比赛",
	network_error: "网络异常：请先恢复赛事",
	finished: "赛事已结束：不能重新部署",
	archived: "赛事已归档：不能重新部署",
};

/** 不可预检时的逐状态原因。 */
const PRECHECK_BLOCK_REASON: Record<string, string> = {
	deploying: "部署进行中：请等待部署结束",
	running: "比赛已开始：仅运行前的配置态可预检",
	paused: "比赛已暂停：仅运行前的配置态可预检",
	network_error: "网络异常：请先恢复赛事",
	finished: "赛事已结束：不能预检",
	archived: "赛事已归档：不能预检",
};

/**
 * 依据后端状态判定每个动作可用性。
 * 判定与后端守卫一一对应：
 * - deploy：`deploy_service` 接受 `is_configurable()` 的状态（幂等重入 Deploying/Deployed）；
 * - precheck：`precheck_service` 要求 is_configurable() 且赛事网络已分配；
 * - start：`event_service::start_event` 要求 status == Verified；
 * - pause：status == Running；resume：status ∈ {Paused, NetworkError}；
 * - finish：`maybe_finish_event` 仅在 Running 且终局结算成立时才真正结束；
 * - archive：`archive_service` 要求 status == Finished；
 * - rotateTokens：需要已配置且赛事网络已分配（要重建 infra 容器）。
 */
export function buildAwdGates({
	status,
	networkAllocated,
	finalSettlement,
}: AwdGateInput): Record<AwdAction, AwdActionGate> {
	const unknown = status === null;
	const networkReason = "赛事网络尚未分配（「网络」标签页）：该操作需要赛事网络与基础设施地址";
	const deploy: AwdActionGate = unknown
		? { enabled: false, reason: "尚未开通 AWD：请先创建 AWD 赛事配置" }
		: !DEPLOYABLE_STATUSES.includes(status)
			? {
					enabled: false,
					reason:
						DEPLOY_BLOCK_REASON[status] ??
						`${statusText(status)}，不可部署；仅运行前的配置态可部署`,
				}
			: !networkAllocated
				? { enabled: false, reason: networkReason }
				: { enabled: true, reason: null };

	const precheck: AwdActionGate = unknown
		? { enabled: false, reason: "尚未开通 AWD：请先创建 AWD 赛事配置" }
		: !CONFIGURABLE_STATUSES.includes(status)
			? {
					enabled: false,
					reason:
						PRECHECK_BLOCK_REASON[status] ??
						`${statusText(status)}，不可预检；仅运行前的配置态可预检`,
				}
			: !networkAllocated
				? { enabled: false, reason: networkReason }
				: { enabled: true, reason: null };

	const start: AwdActionGate = unknown
		? { enabled: false, reason: "尚未开通 AWD" }
		: status !== "verified"
			? {
					enabled: false,
					reason:
						status === "start_blocked"
							? "开赛受阻（start_blocked）：验证通过后配置又被修改，需重新预检通过再开赛"
							: `${statusText(status)}，不可开赛；后端要求预检通过（verified）`,
				}
			: { enabled: true, reason: null };

	const pause: AwdActionGate =
		status === "running"
			? finalSettlement
				? {
						enabled: false,
						reason: "终局结算中：最后一轮已完成，竞赛操作已关闭（等待判题结算后自动结束）",
					}
				: { enabled: true, reason: null }
			: { enabled: false, reason: `${statusText(status)}，只有进行中的比赛可以暂停` };

	const resume: AwdActionGate =
		status === "paused" || status === "network_error"
			? { enabled: true, reason: null }
			: { enabled: false, reason: `${statusText(status)}，只有已暂停或网络异常的赛事可以恢复` };

	const finish: AwdActionGate = unknown
		? { enabled: false, reason: "尚未开通 AWD" }
		: TERMINAL_STATUSES.includes(status)
			? { enabled: false, reason: `${statusText(status)}，已是终态` }
			: status !== "running"
				? { enabled: false, reason: `${statusText(status)}，只有进行中的比赛可以结束` }
				: !finalSettlement
					? {
							enabled: false,
							reason:
								"尚未进入终局结算：需最后一轮已完成且全部判题任务终态；此时调用不会结束赛事",
						}
					: { enabled: true, reason: null };

	const archive: AwdActionGate =
		status === "finished"
			? { enabled: true, reason: null }
			: status === "archived"
				? { enabled: false, reason: "已归档（终态），无需重复归档" }
				: {
						enabled: false,
						reason: `${statusText(status)}，不可归档；归档要求赛事已结束（finished），未通过终局结算时不会成功`,
					};

	const rotateTokens: AwdActionGate =
		status === null
			? { enabled: false, reason: "尚未开通 AWD：无内部令牌可轮换" }
			: TERMINAL_STATUSES.includes(status)
				? { enabled: false, reason: `${statusText(status)}，不再轮换内部令牌` }
				: !networkAllocated
					? { enabled: false, reason: networkReason }
					: { enabled: true, reason: null };

	return { deploy, precheck, start, pause, resume, finish, archive, rotateTokens };
}

/* ── 预检报告 ─────────────────────────────────────────────────────────────── */

export interface PrecheckEntry {
	component: string;
	error?: string;
	note?: string;
}

export interface PrecheckReport {
	errors: PrecheckEntry[];
	notes: PrecheckEntry[];
}

/**
 * `AwdPrecheckRun.error_msg` 是**后端落库的 JSON 字符串**
 * （`{"errors":[{"component","error"}],"notes":[{"component","note"}]}`）。
 * 非法内容按单条错误展示，绝不吞掉失败原因（CONVENTIONS §6.13）。
 */
export function parsePrecheckReport(raw?: string | null): PrecheckReport {
	if (!raw) return { errors: [], notes: [] };
	try {
		const parsed = JSON.parse(raw) as { errors?: PrecheckEntry[]; notes?: PrecheckEntry[] };
		return { errors: parsed.errors ?? [], notes: parsed.notes ?? [] };
	} catch {
		return { errors: [{ component: "precheck", error: raw }], notes: [] };
	}
}

export const PRECHECK_STATUS_LABEL: Record<string, string> = {
	pending: "排队中",
	running: "执行中",
	passed: "通过",
	failed: "未通过",
	error: "执行出错",
};

export function precheckStatusTone(status: string | null | undefined): PillTone {
	if (!status) return "muted";
	if (status === "passed") return "success";
	if (status === "failed" || status === "error") return "danger";
	if (status === "running") return "info";
	return "neutral";
}

/** 乐观锁冲突（后端 409）：配置已被他人修改。 */
export function isConflictError(error: unknown): boolean {
	return isFloatCTFError(error) && error.httpStatus === 409;
}
