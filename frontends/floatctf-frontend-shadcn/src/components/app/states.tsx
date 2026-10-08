/**
 * 三态展示基元 —— 平台级要求：**loading / empty / error 一个都不能少**，且错误必须可见可恢复。
 * 所有列表与详情都应通过 `<QueryState>` 渲染，避免有人漏掉某一种状态。
 * 权限不足（403）与 404 有专门的块，语义不与「空数据」混淆。
 */

import type { ReactNode } from "react";
import { AlertTriangle, Inbox, Loader2, Lock, RefreshCw, SearchX, WifiOff } from "lucide-react";

import { Badge } from "~/components/ui/badge";
import { Button } from "~/components/ui/button";
import {
	Empty,
	EmptyContent,
	EmptyDescription,
	EmptyHeader,
	EmptyMedia,
	EmptyTitle,
} from "~/components/ui/empty";
import { Skeleton } from "~/components/ui/skeleton";
import { errorDetail, errorText, isForbidden, isNetworkError, isNotFound } from "~/api/errors";
import { cn } from "~/lib/utils";

interface QueryLike<T> {
	isPending: boolean;
	isError: boolean;
	error: unknown;
	data: T | undefined;
	refetch: () => void;
}

export interface QueryStateProps<T> {
	query: QueryLike<T>;
	/** 首次加载时的骨架（默认通用加载块）。 */
	skeleton?: ReactNode;
	/** 成功但「空」时的展示；与 `isEmpty` 搭配。 */
	empty?: ReactNode | ((data: T) => ReactNode);
	isEmpty?: (data: T) => boolean;
	errorTitle?: string;
	children: (data: T) => ReactNode;
}

/**
 * 统一处理 pending / error / empty / 成功四态。
 *
 * ```tsx
 * <QueryState query={events} isEmpty={(rows) => rows.length === 0} empty={<EmptyBlock ... />}>
 *   {(rows) => <DataTable ... />}
 * </QueryState>
 * ```
 */
export function QueryState<T>({
	query,
	skeleton,
	empty,
	isEmpty,
	errorTitle,
	children,
}: QueryStateProps<T>): ReactNode {
	if (query.isPending) return <>{skeleton ?? <LoadingBlock />}</>;
	if (query.isError) {
		return <ErrorBlock error={query.error} title={errorTitle} onRetry={() => query.refetch()} />;
	}
	const data = query.data as T;
	if (data === undefined || data === null) {
		if (empty !== undefined) {
			return <>{typeof empty === "function" ? empty(data) : empty}</>;
		}
		return <EmptyBlock title="暂无数据" description="后端未返回内容。" />;
	}
	if (isEmpty?.(data)) {
		if (empty === undefined) return <EmptyBlock title="暂无数据" />;
		return <>{typeof empty === "function" ? empty(data) : empty}</>;
	}
	return <>{children(data)}</>;
}

export function LoadingBlock({ label = "加载中…", className }: { label?: string; className?: string }) {
	return (
		<div
			className={cn(
				"flex min-h-40 flex-col items-center justify-center gap-3 text-muted-foreground",
				className,
			)}
			role="status"
			aria-live="polite"
		>
			<Loader2 className="size-5 animate-spin" />
			<span className="text-sm">{label}</span>
		</div>
	);
}

/** 表格骨架：列数固定，避免加载时布局跳动。 */
export function TableSkeleton({ rows = 6, columns = 4 }: { rows?: number; columns?: number }) {
	return (
		<div className="space-y-2 p-4">
			{Array.from({ length: rows }).map((_, rowIndex) => (
				<div key={rowIndex} className="flex items-center gap-4">
					{Array.from({ length: columns }).map((__, columnIndex) => (
						<Skeleton
							key={columnIndex}
							className={cn("h-5", columnIndex === 0 ? "w-48" : "w-24")}
						/>
					))}
				</div>
			))}
		</div>
	);
}

export interface ErrorBlockProps {
	error: unknown;
	title?: string;
	onRetry?: () => void;
	className?: string;
}

/** 错误块：可见、具体、可重试；网络 / 403 / 404 有专门语义。 */
export function ErrorBlock({ error, title, onRetry, className }: ErrorBlockProps) {
	const detail = errorDetail(error);
	const network = isNetworkError(error);
	const forbidden = isForbidden(error);
	const notFound = isNotFound(error);
	const heading =
		title ??
		(network
			? "网络连接失败"
			: forbidden
				? "没有访问权限"
				: notFound
					? "资源不存在"
					: "加载失败");

	return (
		<div
			className={cn(
				"flex flex-col items-center justify-center gap-3 rounded-lg border border-destructive/30 bg-destructive/5 px-6 py-10 text-center",
				className,
			)}
			role="alert"
		>
			{network ? (
				<WifiOff className="size-6 text-destructive" />
			) : forbidden ? (
				<Lock className="size-6 text-destructive" />
			) : (
				<AlertTriangle className="size-6 text-destructive" />
			)}
			<div className="space-y-1">
				<p className="font-medium">{heading}</p>
				<p className="text-sm text-muted-foreground">{errorText(error)}</p>
			</div>
			{detail ? (
				<details className="max-w-xl text-left">
					<summary className="cursor-pointer text-xs text-muted-foreground">技术细节</summary>
					<pre className="mt-2 overflow-x-auto rounded-md bg-muted p-2 font-mono text-xs">
						{detail}
					</pre>
				</details>
			) : null}
			{onRetry ? (
				<Button variant="outline" size="sm" onClick={onRetry}>
					<RefreshCw className="size-4" />
					重试
				</Button>
			) : null}
		</div>
	);
}

/** 行内错误提示（表单 / 局部区块用，不占整屏）。 */
export function InlineError({ error, className }: { error: unknown; className?: string }) {
	return (
		<p className={cn("flex items-center gap-1.5 text-sm text-destructive", className)} role="alert">
			<AlertTriangle className="size-4 shrink-0" />
			{errorText(error)}
		</p>
	);
}

export interface EmptyBlockProps {
	title?: string;
	description?: ReactNode;
	icon?: ReactNode;
	action?: ReactNode;
	className?: string;
	/** 区分「真的没有」与「筛选后没有」。 */
	variant?: "empty" | "filtered";
}

export function EmptyBlock({
	title,
	description,
	icon,
	action,
	className,
	variant = "empty",
}: EmptyBlockProps) {
	const fallbackTitle = variant === "filtered" ? "没有匹配的结果" : "暂无数据";
	const Media = variant === "filtered" ? SearchX : Inbox;
	return (
		<Empty className={cn("border border-dashed py-10", className)}>
			<EmptyHeader>
				<EmptyMedia variant="icon">{icon ?? <Media className="size-5" />}</EmptyMedia>
				<EmptyTitle>{title ?? fallbackTitle}</EmptyTitle>
				<EmptyDescription>
					{description ??
						(variant === "filtered" ? "试试调整搜索或筛选条件。" : "这里还没有内容。")}
				</EmptyDescription>
				{action ? <EmptyContent>{action}</EmptyContent> : null}
			</EmptyHeader>
		</Empty>
	);
}

export function PermissionDeniedBlock({ description }: { description?: string }) {
	return (
		<Empty className="border border-dashed py-10">
			<EmptyHeader>
				<EmptyMedia variant="icon">
					<Lock className="size-5" />
				</EmptyMedia>
				<EmptyTitle>权限不足</EmptyTitle>
				<EmptyDescription>
					{description ?? "当前账号没有访问该资源的权限，请联系赛事管理员。"}
				</EmptyDescription>
			</EmptyHeader>
		</Empty>
	);
}

export function NotFoundBlock({
	title = "页面不存在",
	description = "地址可能已变更，或该资源已被删除。",
	action,
}: {
	title?: string;
	description?: string;
	action?: ReactNode;
}) {
	return (
		<Empty className="border border-dashed py-14">
			<EmptyHeader>
				<EmptyMedia variant="icon">
					<SearchX className="size-5" />
				</EmptyMedia>
				<EmptyTitle>{title}</EmptyTitle>
				<EmptyDescription>{description}</EmptyDescription>
				{action ? <EmptyContent>{action}</EmptyContent> : null}
			</EmptyHeader>
		</Empty>
	);
}

/** 后台刷新指示（列表已加载、正在 refetch）。 */
export function RefreshingBadge({ active }: { active: boolean }) {
	if (!active) return null;
	return (
		<Badge variant="outline" className="gap-1 text-muted-foreground">
			<Loader2 className="size-3 animate-spin" />
			刷新中
		</Badge>
	);
}
