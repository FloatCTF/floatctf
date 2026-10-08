/**
 * 展示层格式化 —— 只做「后端值 → 人类可读文本」，**不发明任何业务语义**。
 * 后端时间是 RFC3339（带 `Z`，UTC）；展示统一走本地时区。
 */

const dateTimeFormatter = new Intl.DateTimeFormat("zh-CN", {
	year: "numeric",
	month: "2-digit",
	day: "2-digit",
	hour: "2-digit",
	minute: "2-digit",
	hour12: false,
});

const dateTimeSecondsFormatter = new Intl.DateTimeFormat("zh-CN", {
	year: "numeric",
	month: "2-digit",
	day: "2-digit",
	hour: "2-digit",
	minute: "2-digit",
	second: "2-digit",
	hour12: false,
});

const dateFormatter = new Intl.DateTimeFormat("zh-CN", {
	year: "numeric",
	month: "2-digit",
	day: "2-digit",
});

const timeFormatter = new Intl.DateTimeFormat("zh-CN", {
	hour: "2-digit",
	minute: "2-digit",
	hour12: false,
});

export type DateInput = string | number | Date | null | undefined;

export function parseDate(value: DateInput): Date | null {
	if (value === null || value === undefined || value === "") return null;
	const date = value instanceof Date ? value : new Date(value);
	return Number.isNaN(date.getTime()) ? null : date;
}

export function formatDateTime(value: DateInput, opts?: { seconds?: boolean }): string {
	const date = parseDate(value);
	if (!date) return "—";
	return opts?.seconds
		? dateTimeSecondsFormatter.format(date)
		: dateTimeFormatter.format(date);
}

export function formatDate(value: DateInput): string {
	const date = parseDate(value);
	return date ? dateFormatter.format(date) : "—";
}

export function formatTime(value: DateInput): string {
	const date = parseDate(value);
	return date ? timeFormatter.format(date) : "—";
}

export function formatRange(start: DateInput, end: DateInput): string {
	const from = parseDate(start);
	const to = parseDate(end);
	if (!from && !to) return "—";
	if (from && !to) return `${formatDateTime(from)} 起`;
	if (!from && to) return `截至 ${formatDateTime(to)}`;
	return `${formatDateTime(from)} → ${formatDateTime(to)}`;
}

/** 相对时间：用于「最近注册 / 最后更新 / 提交于」这类次要信息。 */
export function formatRelative(value: DateInput, now: Date = new Date()): string {
	const date = parseDate(value);
	if (!date) return "—";
	const diffMs = date.getTime() - now.getTime();
	const abs = Math.abs(diffMs);
	const units: Array<[Intl.RelativeTimeFormatUnit, number]> = [
		["second", 1000],
		["minute", 60 * 1000],
		["hour", 60 * 60 * 1000],
		["day", 24 * 60 * 60 * 1000],
		["month", 30 * 24 * 60 * 60 * 1000],
		["year", 365 * 24 * 60 * 60 * 1000],
	];
	const formatter = new Intl.RelativeTimeFormat("zh-CN", { numeric: "auto" });
	let chosen: [Intl.RelativeTimeFormatUnit, number] = units[0];
	for (const unit of units) {
		if (abs >= unit[1]) chosen = unit;
	}
	if (abs < 45 * 1000) return "刚刚";
	const amount = Math.round(diffMs / chosen[1]);
	return formatter.format(amount, chosen[0]);
}

/** 秒 → `1h 02m 03s` / `12m 30s` / `45s`；用于比赛时长与轮次倒计时。 */
export function formatDuration(seconds: number | null | undefined): string {
	if (seconds === null || seconds === undefined || Number.isNaN(seconds)) return "—";
	const total = Math.max(0, Math.floor(seconds));
	const h = Math.floor(total / 3600);
	const m = Math.floor((total % 3600) / 60);
	const s = total % 60;
	if (h > 0) return `${h}h ${String(m).padStart(2, "0")}m ${String(s).padStart(2, "0")}s`;
	if (m > 0) return `${m}m ${String(s).padStart(2, "0")}s`;
	return `${s}s`;
}

/** 毫秒（评测耗时）→ 人类可读。 */
export function formatMillis(ms: number | null | undefined): string {
	if (ms === null || ms === undefined || Number.isNaN(ms)) return "—";
	if (ms < 1000) return `${Math.round(ms)}ms`;
	return `${(ms / 1000).toFixed(2)}s`;
}

/** 分数：保留 2 位小数（后端分数是浮点，动态分值衰减会产生小数）。 */
export function formatScore(value: number | null | undefined, digits = 2): string {
	if (value === null || value === undefined || Number.isNaN(value)) return "—";
	return value.toFixed(digits);
}

/** 带正负号的分数变动（管理员调分 / 罚分展示）。 */
export function formatDelta(value: number | null | undefined, digits = 2): string {
	if (value === null || value === undefined || Number.isNaN(value)) return "—";
	const sign = value > 0 ? "+" : "";
	return `${sign}${value.toFixed(digits)}`;
}

export function formatInt(value: number | null | undefined): string {
	if (value === null || value === undefined || Number.isNaN(value)) return "—";
	return new Intl.NumberFormat("zh-CN").format(value);
}

export function formatBytes(bytes: number | null | undefined): string {
	if (bytes === null || bytes === undefined || Number.isNaN(bytes)) return "—";
	if (bytes < 1024) return `${bytes} B`;
	const units = ["KB", "MB", "GB", "TB"];
	let value = bytes / 1024;
	let index = 0;
	while (value >= 1024 && index < units.length - 1) {
		value /= 1024;
		index += 1;
	}
	return `${value.toFixed(value < 10 ? 1 : 0)} ${units[index]}`;
}

export function formatPercent(ratio: number | null | undefined, digits = 1): string {
	if (ratio === null || ratio === undefined || Number.isNaN(ratio)) return "—";
	return `${(ratio * 100).toFixed(digits)}%`;
}

export function truncate(text: string, max = 120): string {
	if (text.length <= max) return text;
	return `${text.slice(0, max - 1)}…`;
}

/** `datetime-local` 输入值 ↔ ISO（UTC）。管理端赛事时间表单专用。 */
export function toDatetimeLocalValue(value: DateInput): string {
	const date = parseDate(value);
	if (!date) return "";
	const pad = (n: number) => String(n).padStart(2, "0");
	return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

export function fromDatetimeLocalValue(value: string): string | null {
	if (!value) return null;
	const date = new Date(value);
	return Number.isNaN(date.getTime()) ? null : date.toISOString();
}

/** 倒计时文案（赛事开始 / 结束）。 */
export function countdownText(target: DateInput, now: Date = new Date()): string {
	const date = parseDate(target);
	if (!date) return "—";
	const diff = Math.floor((date.getTime() - now.getTime()) / 1000);
	if (diff <= 0) return "已到时间";
	if (diff < 60) return `${diff} 秒`;
	if (diff < 3600) return `${Math.floor(diff / 60)} 分钟`;
	if (diff < 86400) return `${Math.floor(diff / 3600)} 小时 ${Math.floor((diff % 3600) / 60)} 分`;
	return `${Math.floor(diff / 86400)} 天 ${Math.floor((diff % 86400) / 3600)} 小时`;
}
