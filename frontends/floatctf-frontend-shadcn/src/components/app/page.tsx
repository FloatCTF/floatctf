/**
 * 页面骨架基元 —— 统一本前端的版式语言（shadcn 卡片 + 工具条 + 等宽数值）。
 * 页面级组件只组合这些基元，不各自发明间距与层级。
 */

import { useState, type ReactNode } from "react";
import { Check, ChevronLeft, ChevronRight, Copy, Eye, EyeOff } from "lucide-react";

import { Badge } from "~/components/ui/badge";
import { Button } from "~/components/ui/button";
import {
	Card,
	CardAction,
	CardContent,
	CardDescription,
	CardHeader,
	CardTitle,
} from "~/components/ui/card";
import { ScrollArea } from "~/components/ui/scroll-area";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "~/components/ui/select";
import { Separator } from "~/components/ui/separator";
import { useCopyToClipboard } from "~/lib/hooks";
import { cn } from "~/lib/utils";

export function PageHeader({
	title,
	description,
	actions,
	breadcrumbs,
	badge,
	className,
}: {
	title: ReactNode;
	description?: ReactNode;
	actions?: ReactNode;
	breadcrumbs?: ReactNode;
	badge?: ReactNode;
	className?: string;
}) {
	return (
		<div className={cn("flex flex-col gap-3", className)}>
			{breadcrumbs ? <div className="text-xs text-muted-foreground">{breadcrumbs}</div> : null}
			<div className="flex flex-wrap items-start justify-between gap-3">
				<div className="min-w-0 space-y-1">
					<div className="flex flex-wrap items-center gap-2">
						<h1 className="truncate text-xl font-semibold tracking-tight">{title}</h1>
						{badge}
					</div>
					{description ? (
						<p className="max-w-3xl text-sm text-muted-foreground">{description}</p>
					) : null}
				</div>
				{actions ? <div className="flex flex-wrap items-center gap-2">{actions}</div> : null}
			</div>
		</div>
	);
}

export function PageBody({ children, className }: { children: ReactNode; className?: string }) {
	return (
		<div className={cn("mx-auto w-full max-w-screen-2xl space-y-5 px-4 py-5 lg:px-6", className)}>
			{children}
		</div>
	);
}

export function SectionCard({
	title,
	description,
	actions,
	children,
	className,
	contentClassName,
	footer,
}: {
	title?: ReactNode;
	description?: ReactNode;
	actions?: ReactNode;
	children: ReactNode;
	className?: string;
	contentClassName?: string;
	footer?: ReactNode;
}) {
	return (
		<Card className={cn("overflow-hidden", className)}>
			{title || actions || description ? (
				<CardHeader>
					{title ? <CardTitle className="text-base">{title}</CardTitle> : null}
					{description ? <CardDescription>{description}</CardDescription> : null}
					{actions ? <CardAction>{actions}</CardAction> : null}
				</CardHeader>
			) : null}
			<CardContent className={cn(title ? "" : "pt-5", contentClassName)}>{children}</CardContent>
			{footer ? (
				<>
					<Separator />
					<div className="flex flex-wrap items-center justify-between gap-2 px-5 py-3">
						{footer}
					</div>
				</>
			) : null}
		</Card>
	);
}

export function StatCard({
	label,
	value,
	hint,
	icon,
	tone = "default",
}: {
	label: string;
	value: ReactNode;
	hint?: ReactNode;
	icon?: ReactNode;
	tone?: "default" | "success" | "warning" | "danger";
}) {
	const toneClass =
		tone === "success"
			? "text-[var(--success)]"
			: tone === "warning"
				? "text-[var(--warning)]"
				: tone === "danger"
					? "text-destructive"
					: "text-foreground";
	return (
		<Card>
			<CardContent className="flex items-start justify-between gap-3 pt-5">
				<div className="min-w-0 space-y-1">
					<p className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
						{label}
					</p>
					<p className={cn("tnum text-2xl font-semibold", toneClass)}>{value}</p>
					{hint ? <p className="text-xs text-muted-foreground">{hint}</p> : null}
				</div>
				{icon ? <div className="text-muted-foreground">{icon}</div> : null}
			</CardContent>
		</Card>
	);
}

export function Toolbar({
	children,
	className,
}: {
	children: ReactNode;
	className?: string;
}) {
	return (
		<div className={cn("flex flex-wrap items-center gap-2", className)}>{children}</div>
	);
}

export interface KeyValueItem {
	key: string;
	value: ReactNode;
	hint?: ReactNode;
}

export function KeyValueList({
	items,
	columns = 2,
	className,
}: {
	items: KeyValueItem[];
	columns?: 1 | 2 | 3;
	className?: string;
}) {
	return (
		<dl
			className={cn(
				"grid gap-x-6 gap-y-3",
				columns === 1 ? "grid-cols-1" : columns === 3 ? "sm:grid-cols-3" : "sm:grid-cols-2",
				className,
			)}
		>
			{items.map((item) => (
				<div key={item.key} className="min-w-0 space-y-0.5">
					<dt className="text-xs text-muted-foreground">{item.key}</dt>
					<dd className="text-sm break-words">{item.value}</dd>
					{item.hint ? <p className="text-xs text-muted-foreground">{item.hint}</p> : null}
				</div>
			))}
		</dl>
	);
}

export function MonoText({
	children,
	className,
	title,
}: {
	children: ReactNode;
	className?: string;
	title?: string;
}) {
	return (
		<span className={cn("tnum font-mono text-xs break-all", className)} title={title}>
			{children}
		</span>
	);
}

export function CopyText({
	value,
	label,
	className,
	mono = true,
}: {
	value: string;
	label?: string;
	className?: string;
	mono?: boolean;
}) {
	const { copied, copy } = useCopyToClipboard();
	return (
		<span className={cn("inline-flex min-w-0 items-center gap-1.5", className)}>
			<span className={cn("min-w-0 truncate", mono && "tnum font-mono text-xs")} title={value}>
				{value}
			</span>
			<Button
				variant="ghost"
				size="icon-xs"
				onClick={() => void copy(value)}
				aria-label={copied ? (label ? `${label}已复制` : "已复制") : label ? `复制${label}` : "复制"}
				title={label ? `复制${label}` : "复制"}
			>
				{copied ? <Check className="text-[var(--success)]" /> : <Copy />}
			</Button>
		</span>
	);
}

/** 敏感值（flag / 密码 / 网络密钥）：默认模糊，点「显示」才展开；复制按钮始终可用。 */
export function SecretValue({ value, label }: { value: string; label?: string }) {
	const [shown, setShown] = useState(false);
	return (
		<span className="inline-flex min-w-0 items-center gap-1.5">
			<span
				className={cn(
					"tnum min-w-0 truncate font-mono text-xs",
					!shown && "blur-[3px] select-none",
				)}
				title={shown ? value : (label ?? "点击显示")}
			>
				{value || "—"}
			</span>
			<Button
				variant="ghost"
				size="icon-xs"
				onClick={() => setShown((prev) => !prev)}
				aria-label={shown ? "隐藏" : "显示"}
				title={shown ? "隐藏" : "显示"}
			>
				{shown ? <EyeOff /> : <Eye />}
			</Button>
			{value ? <CopyText value={value} label={label} mono={false} /> : null}
		</span>
	);
}

export function PaginationBar({
	page,
	pageSize,
	total,
	onPageChange,
	onPageSizeChange,
	pageSizeOptions = [10, 20, 50],
}: {
	page: number;
	pageSize: number;
	total: number;
	onPageChange: (page: number) => void;
	onPageSizeChange?: (size: number) => void;
	pageSizeOptions?: number[];
}) {
	const totalPages = Math.max(1, Math.ceil(total / Math.max(1, pageSize)));
	const from = total === 0 ? 0 : (page - 1) * pageSize + 1;
	const to = Math.min(total, page * pageSize);
	return (
		<div className="flex flex-wrap items-center justify-between gap-3">
			<p className="tnum text-xs text-muted-foreground">
				共 {total} 条{total > 0 ? ` · 显示 ${from}–${to}` : ""}
			</p>
			<div className="flex items-center gap-2">
				{onPageSizeChange ? (
					<Select
						value={String(pageSize)}
						onValueChange={(value) => onPageSizeChange(Number(value))}
					>
						<SelectTrigger size="sm" className="w-[110px]" aria-label="每页条数">
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							{pageSizeOptions.map((size) => (
								<SelectItem key={size} value={String(size)}>
									每页 {size} 条
								</SelectItem>
							))}
						</SelectContent>
					</Select>
				) : null}
				<Button
					variant="outline"
					size="icon-sm"
					disabled={page <= 1}
					onClick={() => onPageChange(page - 1)}
					aria-label="上一页"
				>
					<ChevronLeft />
				</Button>
				<span className="tnum text-xs text-muted-foreground">
					{page} / {totalPages}
				</span>
				<Button
					variant="outline"
					size="icon-sm"
					disabled={page >= totalPages}
					onClick={() => onPageChange(page + 1)}
					aria-label="下一页"
				>
					<ChevronRight />
				</Button>
			</div>
		</div>
	);
}

export function TagList({
	items,
	emptyText = "—",
}: {
	items: string[];
	emptyText?: string;
}) {
	if (items.length === 0) return <span className="text-xs text-muted-foreground">{emptyText}</span>;
	return (
		<div className="flex flex-wrap gap-1">
			{items.map((item) => (
				<Badge key={item} variant="outline">
					{item}
				</Badge>
			))}
		</div>
	);
}

/** 长文本（描述 / 规则 / writeup）的只读展示容器。 */
export function ReadonlyBlock({
	children,
	className,
	emptyText = "（空）",
}: {
	children: ReactNode;
	className?: string;
	emptyText?: string;
}) {
	const isEmpty =
		children === null ||
		children === undefined ||
		(typeof children === "string" && children.trim().length === 0);
	return (
		<div
			className={cn(
				"rounded-md border bg-muted/30 px-3 py-2 text-sm break-words whitespace-pre-wrap",
				className,
			)}
		>
			{isEmpty ? <span className="text-muted-foreground">{emptyText}</span> : children}
		</div>
	);
}

export function Scrollable({ children, className }: { children: ReactNode; className?: string }) {
	return <ScrollArea className={cn("max-h-80", className)}>{children}</ScrollArea>;
}
