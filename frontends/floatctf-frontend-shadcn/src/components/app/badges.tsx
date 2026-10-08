/**
 * 状态标签与实时状态指示 —— 视觉上统一，语义由调用方传入（**不在前端自造状态**）。
 */

import type { ReactNode } from "react";
import { CircleDot, Loader2, PlugZap, RefreshCw, WifiOff } from "lucide-react";

import { Button } from "~/components/ui/button";
import { cn } from "~/lib/utils";

export type PillTone = "neutral" | "muted" | "success" | "warning" | "danger" | "info";

const toneClass: Record<PillTone, string> = {
	neutral: "border-border bg-muted text-foreground",
	muted: "border-transparent bg-transparent text-muted-foreground",
	success: "border-transparent bg-[var(--success)]/15 text-[var(--success)]",
	warning: "border-transparent bg-[var(--warning)]/20 text-[var(--warning)]",
	danger: "border-transparent bg-destructive/15 text-destructive",
	info: "border-transparent bg-primary/15 text-primary",
};

export function TonePill({
	tone = "neutral",
	children,
	icon,
	className,
	title,
}: {
	tone?: PillTone;
	children: ReactNode;
	icon?: ReactNode;
	className?: string;
	title?: string;
}) {
	return (
		<span
			title={title}
			className={cn(
				"inline-flex w-fit shrink-0 items-center gap-1 rounded-full border px-2 py-0.5 text-xs font-medium whitespace-nowrap [&_svg]:size-3",
				toneClass[tone],
				className,
			)}
		>
			{icon}
			{children}
		</span>
	);
}

/** SSE 连接状态 → 人类可读 + 颜色（断线必须显式可见，不能静默）。 */
export function RealtimePill({
	state,
	label = "实时",
	onRefresh,
	className,
}: {
	state: string | undefined;
	label?: string;
	onRefresh?: () => void;
	className?: string;
}) {
	const normalized = (state ?? "idle").toLowerCase();
	const map: Record<string, { tone: PillTone; text: string; icon: ReactNode }> = {
		connected: { tone: "success", text: "实时已连接", icon: <PlugZap /> },
		connecting: { tone: "info", text: "连接中…", icon: <Loader2 className="animate-spin" /> },
		reconnecting: {
			tone: "warning",
			text: "重连中（已降级轮询）",
			icon: <RefreshCw className="animate-spin" />,
		},
		auth_error: { tone: "danger", text: "实时鉴权失败（需重新登录）", icon: <WifiOff /> },
		error: { tone: "danger", text: "实时通道异常（已降级轮询）", icon: <WifiOff /> },
		closed: { tone: "muted", text: "实时已断开（已降级轮询）", icon: <WifiOff /> },
		idle: { tone: "muted", text: "实时未启用", icon: <CircleDot /> },
	};
	const entry = map[normalized] ?? {
		tone: "muted" as PillTone,
		text: normalized,
		icon: <CircleDot />,
	};

	return (
		<span className={cn("inline-flex items-center gap-1.5", className)}>
			<TonePill tone={entry.tone} icon={entry.icon} title={`${label}：${normalized}`}>
				{entry.text}
			</TonePill>
			{onRefresh ? (
				<Button variant="ghost" size="icon-xs" onClick={onRefresh} aria-label="立即刷新" title="立即刷新">
					<RefreshCw />
				</Button>
			) : null}
		</span>
	);
}

/** 布尔/可空状态的通用「是 / 否 / 未知」标签（避免各页面自成一套）。 */
export function BooleanPill({
	value,
	trueText = "是",
	falseText = "否",
	trueTone = "success",
	falseTone = "muted",
}: {
	value: boolean | null | undefined;
	trueText?: string;
	falseText?: string;
	trueTone?: PillTone;
	falseTone?: PillTone;
}) {
	if (value === null || value === undefined) return <TonePill tone="muted">未知</TonePill>;
	return (
		<TonePill tone={value ? trueTone : falseTone}>{value ? trueText : falseText}</TonePill>
	);
}
