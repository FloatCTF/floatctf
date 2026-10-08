/**
 * 赛事管理共享件 —— 状态标签、分页表格壳、移动端卡片行。
 *
 * 状态语义全部来自 `~/lib/event-status.ts` 与 SDK 枚举（**不自造状态**）；
 * 表格统一走「服务端 `meta.total` + `PaginationBar`」，或由调用方传入本页总数（后端
 * 不接收分页参数的接口 = 客户端分页，见 `total` 说明）。
 */

import type { ReactNode } from "react";

import type { QueryParams } from "@floatctf/sdk";
import { EventPurpose } from "@floatctf/sdk/entity";
import type { Events } from "@floatctf/sdk/entity";

import { TonePill } from "~/components/app/badges";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { PaginationBar } from "~/components/app/page";
import { EmptyBlock, QueryState, TableSkeleton } from "~/components/app/states";
import {
	EVENT_FAMILY_LABEL,
	EVENT_PURPOSE_LABEL,
	EVENT_STATUS_LABEL,
	EVENT_STATUS_TONE,
	PARTICIPANT_MODE_LABEL,
	computeEventStatus,
} from "~/lib/event-status";

/* ── 状态标签 ───────────────────────────────────────────────────────────── */

export function EventStatusPill({ event }: { event: Events }): ReactNode {
	const status = computeEventStatus(event.start_time, event.end_time ?? null);
	return <TonePill tone={EVENT_STATUS_TONE[status]}>{EVENT_STATUS_LABEL[status]}</TonePill>;
}

export function EventFamilyPill({ family }: { family: string }): ReactNode {
	return <TonePill tone="neutral">{EVENT_FAMILY_LABEL[family] ?? family}</TonePill>;
}

export function ParticipantModePill({ mode }: { mode: string }): ReactNode {
	return <TonePill tone="info">{PARTICIPANT_MODE_LABEL[mode] ?? mode}</TonePill>;
}

/** 用途 + 虚拟赛事位（`is_virtual` 由后端按 `purpose=practice` 派生）。 */
export function EventPurposePills({ event }: { event: Events }): ReactNode {
	return (
		<span className="inline-flex flex-wrap items-center gap-1">
			<TonePill tone={event.purpose === EventPurpose.Practice ? "warning" : "muted"}>
				{EVENT_PURPOSE_LABEL[event.purpose] ?? event.purpose}
			</TonePill>
			{event.is_virtual ? <TonePill tone="muted">虚拟赛事</TonePill> : null}
		</span>
	);
}

/* ── 分页表格壳 ─────────────────────────────────────────────────────────── */

/** 一条「已分页的列表查询」的最小结构（与 TanStack `useQuery` 返回结构兼容）。 */
export interface PagedResult<T> {
	items: T[];
	meta: QueryParams;
}

export interface PagedQuery<T> {
	isPending: boolean;
	isError: boolean;
	error: unknown;
	data: PagedResult<T> | undefined;
	refetch: () => void;
}

export interface PagedTableProps<T> {
	query: PagedQuery<T>;
	columns: DataTableColumn<T>[];
	getRowId: (row: T, index: number) => string;
	page: number;
	pageSize: number;
	onPageChange: (page: number) => void;
	onPageSizeChange?: (size: number) => void;
	/**
	 * 本页总数。省略时取 `meta.total`（服务端分页）；后端 **忽略** page/limit 的接口
	 * （如赛事实例、战队）请传 `全量条数` 走客户端分页。
	 */
	total?: number;
	rowActions?: (row: T) => ReactNode;
	selectable?: boolean;
	selectedIds?: string[];
	onSelectionChange?: (ids: string[]) => void;
	mobileCard?: (row: T) => ReactNode;
	empty?: ReactNode;
	errorTitle?: string;
}

export function PagedTable<T>({
	query,
	columns,
	getRowId,
	page,
	pageSize,
	onPageChange,
	onPageSizeChange,
	total,
	rowActions,
	selectable,
	selectedIds,
	onSelectionChange,
	mobileCard,
	empty,
	errorTitle,
}: PagedTableProps<T>): ReactNode {
	const resolvedTotal = total ?? query.data?.meta?.total ?? query.data?.items.length ?? 0;
	return (
		<QueryState<PagedResult<T>>
			query={query}
			errorTitle={errorTitle}
			skeleton={<TableSkeleton rows={5} columns={Math.min(Math.max(columns.length, 3), 6)} />}
			isEmpty={(result) => result.items.length === 0}
			empty={empty ?? <EmptyBlock title="暂无数据" description="后端未返回任何条目。" />}
		>
			{(result) => (
				<div className="space-y-3">
					<DataTable
						data={result.items}
						columns={columns}
						getRowId={getRowId}
						rowActions={rowActions}
						selectable={selectable}
						selectedIds={selectedIds}
						onSelectionChange={onSelectionChange}
						mobileCard={mobileCard}
					/>
					<PaginationBar
						page={page}
						pageSize={pageSize}
						total={resolvedTotal}
						onPageChange={onPageChange}
						onPageSizeChange={onPageSizeChange}
					/>
				</div>
			)}
		</QueryState>
	);
}

/**
 * 客户端分页：后端 **忽略** `page`/`limit` 的接口（`event_teams.getTeams`、`instances.listForEvent`）
 * 会一次返回全量数据，这里只对当前页做切片，`meta.total` 用全量条数补齐。
 */
export function clientPaged<T>(
	query: PagedQuery<T>,
	page: number,
	pageSize: number,
): PagedQuery<T> {
	const data = query.data;
	if (!data) return query;
	const start = (page - 1) * pageSize;
	return {
		...query,
		data: {
			items: data.items.slice(start, start + pageSize),
			meta: { ...data.meta, total: data.meta.total ?? data.items.length },
		},
	};
}

/** 客户端分页时的安全页码（删除后页数会减少）。 */
export function clientPageCount(total: number, pageSize: number): number {
	return Math.max(1, Math.ceil(total / Math.max(1, pageSize)));
}

/* ── 移动端卡片 ─────────────────────────────────────────────────────────── */
/** 移动端退化卡片里的「字段 / 值」对齐行（`DataTable` 的 `mobileCard` 用它拼装）。 */
export function MobileFacts({
	items,
}: {
	items: Array<{ k: string; v: ReactNode }>;
}): ReactNode {
	return (
		<div className="space-y-1.5">
			{items.map((item) => (
				<div key={item.k} className="flex items-start justify-between gap-3">
					<span className="shrink-0 text-xs text-muted-foreground">{item.k}</span>
					<span className="min-w-0 text-right text-sm break-words">{item.v}</span>
				</div>
			))}
		</div>
	);
}
