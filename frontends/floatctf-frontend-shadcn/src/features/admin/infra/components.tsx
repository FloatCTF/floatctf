/**
 * 基础设施域内的共用基元。
 */

import { useEffect, useState, type ReactNode } from "react";
import { Search } from "lucide-react";

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

/**
 * 分页 + 搜索状态。
 *
 * 本域 Docker 三个列表都用 `offset`/`limit` 做**服务端**分页，但后端 Docker 接口
 * 没有任何 filter 映射，所以搜索只在**当前页**内生效 —— 页面必须显式说明这一点，
 * 不能假装是全局搜索。
 */
export function useListState(initialPageSize = 20): {
	page: number;
	setPage: (page: number) => void;
	pageSize: number;
	setPageSize: (pageSize: number) => void;
	search: string;
	setSearch: (search: string) => void;
	filter: string;
} {
	const [page, setPage] = useState(1);
	const [pageSize, setPageSizeState] = useState(initialPageSize);
	const [search, setSearch] = useState("");
	const filter = useDebouncedValue(search.trim(), 250);

	useEffect(() => {
		setPage(1);
	}, [filter]);

	function setPageSize(next: number) {
		setPageSizeState(next);
		setPage(1);
	}

	return { page, setPage, pageSize, setPageSize, search, setSearch, filter };
}

/** Docker 的 `created` 是 Unix 秒；转成可展示的 ISO 字符串。 */
export function fromUnixSeconds(seconds: number | null | undefined): string | null {
	if (typeof seconds !== "number" || !Number.isFinite(seconds) || seconds <= 0) return null;
	return new Date(seconds * 1000).toISOString();
}

/** 当前页内过滤（后端无 filter 支持时的诚实做法）。 */
export function filterCurrentPage<T>(
	rows: T[],
	keyword: string,
	values: (row: T) => string[],
): T[] {
	if (keyword.length === 0) return rows;
	const lowered = keyword.toLowerCase();
	return rows.filter((row) => values(row).some((value) => value.toLowerCase().includes(lowered)));
}
