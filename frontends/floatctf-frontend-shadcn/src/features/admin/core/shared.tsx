/**
 * 管理端核心域的内部工具（列表状态 / 服务端过滤 / 数字与 JSON 输入 / 结果面板）。
 *
 * 这里**只**放本域复用的纯逻辑与小组件，不放任何业务数据：所有展示值都来自页面里的真实接口。
 */

import { useCallback, useState, type ReactNode } from "react";
import { MoreHorizontal, Search, X } from "lucide-react";

import { SectionCard } from "~/components/app/page";
import { TonePill } from "~/components/app/badges";
import { Button } from "~/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuTrigger,
} from "~/components/ui/dropdown-menu";
import { Input } from "~/components/ui/input";
import { useDebouncedValue } from "~/lib/hooks";
import { cn } from "~/lib/utils";

// ── 服务端过滤（平台 `filter` 微语法：`key:value | key:value`）──────────────

/**
 * 把单框搜索词翻译成平台 `filter` 字符串。
 *
 * 平台语法（`apps/api` 的 `build_filter_condition`）：
 * - `key:value` 单条件；`&` 表示 AND，`|` 表示 OR；
 * - 未映射的 key 被后端**静默忽略**（全忽略时不做任何过滤）；
 * - 连续 token 会并入上一个 value，因此搜索词可含空格。
 *
 * 搜索词里的 `&` / `|` / `:` 会被清掉，避免用户输入被当成过滤语法注入。
 */
export function buildSearchFilter(keys: string[], query: string): string | undefined {
	const cleaned = query
		.replace(/[&|:]/g, " ")
		.replace(/\s+/g, " ")
		.trim();
	if (!cleaned) return undefined;
	return keys.map((key) => `${key}:${cleaned}`).join(" | ");
}

// ── 列表状态（page 从 1 开始 + pageSize + 去抖搜索）───────────────────────

export interface AdminListState {
	page: number;
	pageSize: number;
	/** 输入框里的原始值（受控）。 */
	search: string;
	/** 去抖后翻译出的服务端过滤串；空搜索为 undefined。 */
	filter: string | undefined;
	setPage: (page: number) => void;
	setPageSize: (size: number) => void;
	setSearch: (value: string) => void;
}

/**
 * 列表页统一状态。搜索去抖 300ms；改搜索词或每页条数时**回到第 1 页**，
 * 否则用户会停在一个超出结果集的页码上看到空列表。
 */
export function useAdminListState(searchKeys: string[], initialPageSize = 20): AdminListState {
	const [page, setPage] = useState(1);
	const [pageSize, setPageSizeState] = useState(initialPageSize);
	const [search, setSearchState] = useState("");
	const debounced = useDebouncedValue(search, 300);
	const filter = buildSearchFilter(searchKeys, debounced);

	const setSearch = useCallback((value: string) => {
		setSearchState(value);
		setPage(1);
	}, []);
	const setPageSize = useCallback((size: number) => {
		setPageSizeState(size);
		setPage(1);
	}, []);

	return { page, pageSize, search, filter, setPage, setPageSize, setSearch };
}

/** 列表页搜索框（图标 + 去抖由调用方负责）。 */
export function SearchInput({
	value,
	onChange,
	placeholder,
	className,
}: {
	value: string;
	onChange: (value: string) => void;
	placeholder: string;
	className?: string;
}) {
	return (
		<div className={cn("relative w-full sm:max-w-xs", className)}>
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

// ── 数字输入解析（区分「空」与「非法」，否则无法给出可见校验）──────────────

export type IntInput = { kind: "empty" } | { kind: "ok"; value: number } | { kind: "invalid" };

/** 解析整数输入；空串 → empty，非整数 → invalid。 */
export function parseIntInput(text: string): IntInput {
	const trimmed = text.trim();
	if (trimmed === "") return { kind: "empty" };
	if (!/^-?\d+$/.test(trimmed)) return { kind: "invalid" };
	const value = Number(trimmed);
	if (!Number.isSafeInteger(value)) return { kind: "invalid" };
	return { kind: "ok", value };
}

// ── JSON 字段（GameBox 的 healthchecks_json / judge_args_json）──────────────

/**
 * 读：后端返回 `unknown | null`，转成可编辑文本；null/undefined → 空串（= 清空态）。
 * 写：后端要求 JSON **字符串**，显式 `null` 表示清空 —— 由页面负责组装。
 */
export function formatJsonText(value: unknown): string {
	if (value === null || value === undefined) return "";
	if (typeof value === "string") return value;
	try {
		return JSON.stringify(value, null, 2) ?? String(value);
	} catch {
		return String(value);
	}
}

export type JsonInput = { ok: true; value: unknown } | { ok: false; error: string };

/** 校验 JSON 文本；失败时把解析器错误文案交给 `<Field error>` 显示。 */
export function parseJsonInput(text: string): JsonInput {
	try {
		return { ok: true, value: JSON.parse(text) as unknown };
	} catch (error) {
		return { ok: false, error: error instanceof Error ? error.message : "JSON 解析失败" };
	}
}

// ── 构建状态 ────────────────────────────────────────────────────────────────

/**
 * `build_status` 标签。后端只会写 `building` / `ready` / `failed`
 *（`import_service::BUILD_STATUS_*`），其它值原样展示而不是硬塞进某个色调。
 */
export function BuildStatusPill({ status }: { status?: string | null }) {
	if (!status) return <span className="text-xs text-muted-foreground">未构建</span>;
	const tone =
		status === "ready"
			? "success"
			: status === "building"
				? "info"
				: status === "failed"
					? "danger"
					: "muted";
	return (
		<TonePill tone={tone} title={`build_status = ${status}`}>
			{status}
		</TonePill>
	);
}

// ── 行内操作菜单 ────────────────────────────────────────────────────────────

/** 行操作统一收进下拉菜单（`DataTable.rowActions` 直接返回它）。 */
export function RowMenu({
	label,
	children,
}: {
	label: string;
	children: ReactNode;
}) {
	return (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button variant="ghost" size="icon-sm" aria-label={label} title={label}>
					<MoreHorizontal />
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end" className="w-44">
				{children}
			</DropdownMenuContent>
		</DropdownMenu>
	);
}

// ── 批量操作结果面板（扫描 / 校验 / 构建的如实回显）────────────────────────

/**
 * 一次性操作结果面板。**必须**展示后端返回的每一条记录（含失败原因），
 * 而不是只弹一句「成功」——否则失败会被静默吞掉。
 */
export function ResultsCard({
	title,
	description,
	children,
	onDismiss,
}: {
	title: ReactNode;
	description?: ReactNode;
	children: ReactNode;
	onDismiss: () => void;
}) {
	return (
		<SectionCard
			title={title}
			description={description}
			contentClassName="p-0"
			actions={
				<Button variant="ghost" size="sm" onClick={onDismiss}>
					<X /> 关闭
				</Button>
			}
		>
			{children}
		</SectionCard>
	);
}
