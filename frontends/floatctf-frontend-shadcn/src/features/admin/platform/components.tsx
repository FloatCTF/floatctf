/**
 * 平台治理域内的列表基元（与 `features/admin/community/components.tsx` 同构）。
 * 共享层 `~/components/app/*` 提供通用基元，这里只固化本域反复出现的组合。
 */

import { useEffect, useState, type ReactNode } from "react";
import { Loader2, Search, Trash2 } from "lucide-react";

import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import { useDebouncedValue } from "~/lib/hooks";
import { cn } from "~/lib/utils";

export function SearchInput({
	value,
	onChange,
	placeholder = "搜索",
	className,
}: {
	value: string;
	onChange: (value: string) => void;
	placeholder?: string;
	className?: string;
}): ReactNode {
	return (
		<div className={cn("relative w-full sm:max-w-72", className)}>
			<Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
			<Input
				value={value}
				onChange={(event) => onChange(event.target.value)}
				placeholder={placeholder}
				aria-label={placeholder}
				className="pl-8"
			/>
		</div>
	);
}

export interface AdminListState {
	page: number;
	setPage: (page: number) => void;
	pageSize: number;
	setPageSize: (pageSize: number) => void;
	search: string;
	setSearch: (search: string) => void;
	filter: string;
}

export function useListState(initialPageSize = 20): AdminListState {
	const [page, setPage] = useState(1);
	const [pageSize, setPageSizeState] = useState(initialPageSize);
	const [search, setSearch] = useState("");
	const filter = useDebouncedValue(search.trim(), 300);

	useEffect(() => {
		setPage(1);
	}, [filter]);

	function setPageSize(next: number) {
		setPageSizeState(next);
		setPage(1);
	}

	return { page, setPage, pageSize, setPageSize, search, setSearch, filter };
}

export function pageSlice<T>(items: T[], page: number, pageSize: number): T[] {
	const start = (Math.max(1, page) - 1) * pageSize;
	return items.slice(start, start + pageSize);
}

export function DeleteSelectedButton({
	count,
	isPending = false,
	onClick,
	label = "删除所选",
}: {
	count: number;
	isPending?: boolean;
	onClick: () => void;
	label?: string;
}): ReactNode {
	return (
		<Button
			variant="destructive"
			size="sm"
			disabled={count === 0 || isPending}
			onClick={onClick}
		>
			{isPending ? <Loader2 className="animate-spin" /> : <Trash2 />}
			{label}
			{count > 0 ? `（${count}）` : ""}
		</Button>
	);
}

/** 页码越界（删除末页条目）时的兜底块。 */
export function OutOfRangeBlock({ onReset }: { onReset: () => void }): ReactNode {
	return (
		<div className="space-y-3 rounded-lg border border-dashed px-4 py-8 text-center">
			<p className="text-sm font-medium">当前页没有数据</p>
			<p className="text-xs text-muted-foreground">条目可能刚被删除，翻页状态需要重置。</p>
			<Button size="sm" variant="outline" onClick={onReset}>
				回到第 1 页
			</Button>
		</div>
	);
}

/**
 * 安全地把「实际是 JSON 值、但 SDK 类型声明为 string」的字段渲染成文本。
 * （例如 `logs.details`、`scheduled_tasks.payload` —— 后端是 `serde_json::Value`。）
 */
export function jsonText(value: unknown): string {
	if (value === null || value === undefined) return "";
	if (typeof value === "string") return value;
	try {
		return JSON.stringify(value, null, 2);
	} catch {
		return String(value);
	}
}

/** 兼容 number / boolean / JSON 值的短文本（缺失时返回 `—`）。 */
export function textOf(value: unknown): string {
	if (value === null || value === undefined || value === "") return "—";
	if (typeof value === "string") return value;
	if (typeof value === "number" || typeof value === "boolean") return String(value);
	return jsonText(value);
}

/** 过滤串里的 `&` / `|` 是后端 filter 语法的逻辑分隔符，搜索值必须剔除。 */
export function sanitizeFilterValue(value: string): string {
	return value.replace(/[&|]/g, " ").trim();
}
