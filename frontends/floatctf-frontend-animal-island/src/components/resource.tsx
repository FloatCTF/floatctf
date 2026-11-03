/**
 * 列表页骨架。
 *
 * 存在的理由：这个前端有十几个"筛选 + 表格 + 分页"的页面。如果每页各写一遍，
 * 三态处理、分页边界、空态文案很快就不一致 —— 而平台级要求是**每页都必须**
 * 覆盖 loading / empty / error。把它们收进一个组件，页面只需要提供列定义与取数。
 *
 * 分页刻意做成**服务端分页**：表格自身的 `pagination` 是客户端分页，
 * 对本平台的数据量（日志、用户、题目）会在首屏就把全部数据拉下来。
 */
import type { ReactNode } from "react";
import { useState } from "react";
import { Card, Pagination, Table, type TableColumn } from "animal-island-ui";

import { EmptyState, ErrorState, SkeletonCards } from "~/components/states";

export interface ResourcePageProps<T> {
	/** 表格列定义（透传给库的 Table）。 */
	columns: TableColumn<T>[];
	rows: T[] | undefined;
	isLoading: boolean;
	error: unknown;
	refetch?: () => void;
	total?: number;
	pageSize?: number;
	/** 空态文案（各页语境不同）。 */
	emptyTitle?: string;
	emptyDescription?: ReactNode;
	/** 空态下的行动按钮（如"新建"）。 */
	emptyAction?: ReactNode;
	/** 筛选条：放在表格上方，由页面自己提供控件。 */
	filterBar?: ReactNode;
	/** 标题区右侧操作。 */
	actions?: ReactNode;
	rowKey: string | ((row: T) => string);
	disablePagination?: boolean;
}

export function ResourcePage<T extends object>({
	columns,
	rows,
	isLoading,
	error,
	refetch,
	total,
	pageSize = 20,
	emptyTitle = "还没有数据",
	emptyDescription,
	emptyAction,
	filterBar,
	actions,
	rowKey,
	disablePagination,
}: ResourcePageProps<T>) {
	const [page, setPage] = useState(1);
	const [size, setSize] = useState(pageSize);

	// 服务端分页：这里对已取回的数据切片只是**渲染切片**，
	// 真正的翻页由调用方通过 `total` + 重新取数完成。
	const paged = disablePagination
		? (rows ?? [])
		: (rows ?? []).slice((page - 1) * size, page * size);

	return (
		<Card>
			<div className="ai-col" style={{ gap: 12 }}>
				{filterBar || actions ? (
					<div className="ai-row-between">
						<div className="ai-row">{filterBar}</div>
						<div className="ai-row">{actions}</div>
					</div>
				) : null}

				{isLoading ? (
					<SkeletonCards count={4} />
				) : error ? (
					<ErrorState error={error} onRetry={refetch} />
				) : (rows?.length ?? 0) === 0 ? (
					<EmptyState
						title={emptyTitle}
						description={emptyDescription}
						action={emptyAction}
					/>
				) : (
					<>
						<div className="ai-scroll">
							<Table
								columns={columns as never}
								dataSource={paged as never[]}
								rowKey={rowKey as never}
							/>
						</div>
						{!disablePagination && (total ?? rows?.length ?? 0) > size ? (
							<div className="ai-row" style={{ justifyContent: "flex-end" }}>
								<Pagination
									total={total ?? rows?.length ?? 0}
									current={page}
									pageSize={size}
									showTotal
									showSizeChanger
									onChange={(p, s) => {
										setPage(p);
										setSize(s);
									}}
								/>
							</div>
						) : null}
					</>
				)}
			</div>
		</Card>
	);
}

/** 统一的"统计数字卡"，避免每页各拼一遍。 */
export function StatCard({
	label,
	value,
	icon,
	hint,
}: {
	label: string;
	value: ReactNode;
	icon?: ReactNode;
	hint?: string;
}) {
	return (
		<Card>
			<div className="ai-col" style={{ gap: 4 }}>
				<div className="ai-row" style={{ gap: 8 }}>
					{icon}
					<span className="ai-muted">{label}</span>
				</div>
				<div className="ai-stat-value">{value}</div>
				{hint ? (
					<span className="ai-muted" style={{ fontSize: 12 }}>
						{hint}
					</span>
				) : null}
			</div>
		</Card>
	);
}
