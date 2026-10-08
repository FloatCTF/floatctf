/**
 * 社区治理域内的列表基元。
 *
 * 与 `src/components/app/*` 的关系：共享层提供**通用**基元（PageHeader / DataTable /
 * QueryState…），这里只放本域反复出现的组合（搜索框、列表分页状态、批量删除按钮），
 * 不复制共享层已有的能力。
 */

import { useEffect, useState, type ReactNode } from "react";
import { Loader2, Search, Trash2 } from "lucide-react";

import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import { useDebouncedValue } from "~/lib/hooks";
import { cn } from "~/lib/utils";

/** 列表页搜索框（受控；防抖在 `useListState` 里做）。 */
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
	/** 输入框原值（立即回显）。 */
	search: string;
	setSearch: (search: string) => void;
	/** 防抖后的过滤串 —— 直接进 `QueryParams.filter`。 */
	filter: string;
}

/**
 * 列表页的分页 + 搜索状态。
 * `filter` 是防抖后的 `trim()` 结果；过滤条件变化时自动回到第 1 页。
 */
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

/** 客户端分页切片（后端不支持分页的列表用）。 */
export function pageSlice<T>(items: T[], page: number, pageSize: number): T[] {
	const start = (Math.max(1, page) - 1) * pageSize;
	return items.slice(start, start + pageSize);
}

/**
 * 后端 `filter` 语法用 `&` / `|` 作为逻辑分隔符；搜索词必须剔除这两个字符，
 * 否则用户输入会改变过滤表达式的结构。
 */
export function sanitizeFilterValue(value: string): string {
	return value.replace(/[&|]/g, " ").trim();
}

/** 批量删除按钮：显示已选数量，空选禁用。 */
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
