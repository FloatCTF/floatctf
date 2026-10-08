/**
 * 用户 / 战队单元与相对时间 —— 列表高频复用的小组件。
 * 安全：**绝不渲染 `password`**；只使用 username / nickname / avatar。
 */

import type { ReactNode } from "react";

import { Avatar, AvatarFallback, AvatarImage } from "~/components/ui/avatar";
import { formatRelative } from "~/lib/format";
import { useNow } from "~/lib/hooks";
import { cn } from "~/lib/utils";

export function UserAvatar({
	name,
	avatar,
	size = "default",
	className,
}: {
	name: string;
	avatar?: string | null;
	size?: "sm" | "default" | "lg";
	className?: string;
}) {
	const initials = name.trim().slice(0, 2).toUpperCase() || "?";
	const sizeClass = size === "sm" ? "size-6" : size === "lg" ? "size-10" : "size-8";
	return (
		<Avatar className={cn(sizeClass, className)}>
			{avatar ? <AvatarImage src={avatar} alt={name} /> : null}
			<AvatarFallback className="text-[10px]">{initials}</AvatarFallback>
		</Avatar>
	);
}

export function UserCell({
	username,
	nickname,
	avatar,
	secondary = "username",
	className,
}: {
	username: string;
	nickname?: string | null;
	avatar?: string | null;
	/** 主行显示什么，副行显示什么。 */
	secondary?: "username" | "none";
	className?: string;
}): ReactNode {
	const primary = nickname && nickname.length > 0 ? nickname : username;
	return (
		<div className={cn("flex min-w-0 items-center gap-2", className)}>
			<UserAvatar name={primary} avatar={avatar} size="sm" />
			<div className="min-w-0">
				<p className="truncate text-sm font-medium">{primary}</p>
				{secondary === "username" ? (
					<p className="tnum truncate font-mono text-xs text-muted-foreground">{username}</p>
				) : null}
			</div>
		</div>
	);
}

export function RelativeTime({ value, className }: { value: string | null | undefined; className?: string }): ReactNode {
	const now = useNow(30_000);
	return (
		<span className={cn("text-xs text-muted-foreground", className)} title={value ?? undefined}>
			{formatRelative(value, now)}
		</span>
	);
}

export function AbsoluteTime({
	value,
	className,
	seconds = false,
}: {
	value: string | null | undefined;
	className?: string;
	seconds?: boolean;
}): ReactNode {
	return (
		<span className={cn("tnum text-xs", className)} title={value ?? undefined}>
			{value ? formatDateTimeLocal(value, seconds) : "—"}
		</span>
	);
}

function formatDateTimeLocal(value: string, seconds: boolean): string {
	const date = new Date(value);
	if (Number.isNaN(date.getTime())) return value;
	return date.toLocaleString("zh-CN", {
		year: "numeric",
		month: "2-digit",
		day: "2-digit",
		hour: "2-digit",
		minute: "2-digit",
		...(seconds ? { second: "2-digit" as const } : {}),
		hour12: false,
	});
}
