/**
 * 通用数据表（TanStack Table v8 + shadcn Table）。
 *
 * 分页与过滤由**服务端**负责（平台列表接口返回 `meta`），因此这里不做分页模型；
 * 排序是**当前页内**的客户端排序（纯展示便利，不改变后端语义）。
 * 移动端（< md）可用 `mobileCard` 退化为卡片列表。
 */

import { useMemo, useState, type ReactNode } from "react";
import {
	flexRender,
	getCoreRowModel,
	getSortedRowModel,
	useReactTable,
	type ColumnDef,
	type SortingState,
} from "@tanstack/react-table";
import { ArrowDown, ArrowUp, ChevronsUpDown } from "lucide-react";

import { Checkbox } from "~/components/ui/checkbox";
import {
	Table,
	TableBody,
	TableCell,
	TableHead,
	TableHeader,
	TableRow,
} from "~/components/ui/table";
import { cn } from "~/lib/utils";

export interface DataTableColumn<T> {
	id: string;
	header: ReactNode;
	cell: (row: T) => ReactNode;
	/** 提供即允许点击表头排序（比较当前页数据）。 */
	sortValue?: (row: T) => string | number;
	align?: "left" | "right" | "center";
	className?: string;
	headerClassName?: string;
	/** 在窄屏隐藏该列（表格视图）。 */
	hideBelow?: "sm" | "md" | "lg" | "xl";
}

export interface DataTableProps<T> {
	columns: DataTableColumn<T>[];
	data: T[];
	getRowId: (row: T, index: number) => string;
	onRowClick?: (row: T) => void;
	rowActions?: (row: T) => ReactNode;
	selectable?: boolean;
	selectedIds?: string[];
	onSelectionChange?: (ids: string[]) => void;
	mobileCard?: (row: T) => ReactNode;
	className?: string;
	/** 表格主体为空时的展示（通常由外部 `QueryState` 处理，这里做兜底）。 */
	empty?: ReactNode;
}

const hideClass: Record<NonNullable<DataTableColumn<unknown>["hideBelow"]>, string> = {
	sm: "hidden sm:table-cell",
	md: "hidden md:table-cell",
	lg: "hidden lg:table-cell",
	xl: "hidden xl:table-cell",
};

const alignClass = {
	left: "text-left",
	right: "text-right",
	center: "text-center",
} as const;

export function DataTable<T>({
	columns,
	data,
	getRowId,
	onRowClick,
	rowActions,
	selectable = false,
	selectedIds,
	onSelectionChange,
	mobileCard,
	className,
	empty,
}: DataTableProps<T>) {
	const [sorting, setSorting] = useState<SortingState>([]);

	const tableColumns = useMemo<ColumnDef<T>[]>(() => {
		const defs: ColumnDef<T>[] = [];

		if (selectable) {
			defs.push({
				id: "__select",
				header: ({ table }) => (
					<Checkbox
						checked={
							table.getIsAllPageRowsSelected()
								? true
								: table.getIsSomePageRowsSelected()
									? "indeterminate"
									: false
						}
						onCheckedChange={(value) => table.toggleAllPageRowsSelected(value === true)}
						aria-label="全选本页"
					/>
				),
				cell: ({ row }) => (
					<Checkbox
						checked={row.getIsSelected()}
						onCheckedChange={(value) => row.toggleSelected(value === true)}
						aria-label="选择该行"
						onClick={(event) => event.stopPropagation()}
					/>
				),
				enableSorting: false,
				size: 36,
			});
		}

		for (const column of columns) {
			defs.push({
				id: column.id,
				accessorFn: (row: T) => (column.sortValue ? column.sortValue(row) : ""),
				header: () => column.header,
				cell: ({ row }) => column.cell(row.original),
				enableSorting: Boolean(column.sortValue),
				meta: { column },
			});
		}

		if (rowActions) {
			defs.push({
				id: "__actions",
				header: () => <span className="sr-only">操作</span>,
				cell: ({ row }) => (
					<div
						className="flex items-center justify-end gap-1"
						onClick={(event) => event.stopPropagation()}
						onKeyDown={(event) => event.stopPropagation()}
					>
						{rowActions(row.original)}
					</div>
				),
				enableSorting: false,
			});
		}

		return defs;
	}, [columns, rowActions, selectable]);

	const table = useReactTable({
		data,
		columns: tableColumns,
		state: { sorting },
		onSortingChange: setSorting,
		getCoreRowModel: getCoreRowModel(),
		getSortedRowModel: getSortedRowModel(),
		getRowId: (row, index) => getRowId(row, index),
		enableRowSelection: selectable,
		...(selectable && selectedIds && onSelectionChange
			? {
					state: { sorting, rowSelection: Object.fromEntries(selectedIds.map((id) => [id, true])) },
					onRowSelectionChange: (updater) => {
						const next =
							typeof updater === "function"
								? updater(Object.fromEntries(selectedIds.map((id) => [id, true])))
								: updater;
						onSelectionChange(
							Object.entries(next)
								.filter(([, value]) => value)
								.map(([id]) => id),
						);
					},
				}
			: {}),
	});

	const rows = table.getRowModel().rows;

	return (
		<div className={cn("w-full", className)}>
			{mobileCard ? (
				<div className="space-y-2 p-3 md:hidden">
					{rows.length === 0 ? (
						(empty ?? <p className="py-6 text-center text-sm text-muted-foreground">暂无数据</p>)
					) : (
						rows.map((row) => (
							<div key={row.id} className="rounded-lg border p-3">
								{mobileCard(row.original)}
							</div>
						))
					)}
				</div>
			) : null}

			<div className={cn("overflow-x-auto", mobileCard && "hidden md:block")}>
				<Table>
					<TableHeader>
						{table.getHeaderGroups().map((headerGroup) => (
							<TableRow key={headerGroup.id} className="hover:bg-transparent">
								{headerGroup.headers.map((header) => {
									const meta = header.column.columnDef.meta as
										| { column?: DataTableColumn<T> }
										| undefined;
									const column = meta?.column;
									const canSort = header.column.getCanSort();
									const sorted = header.column.getIsSorted();
									return (
										<TableHead
											key={header.id}
											className={cn(
												column?.align ? alignClass[column.align] : "text-left",
												column?.hideBelow ? hideClass[column.hideBelow] : "",
												column?.headerClassName,
											)}
										>
											{canSort ? (
												<button
													type="button"
													className="inline-flex items-center gap-1 text-xs font-medium hover:text-foreground"
													onClick={header.column.getToggleSortingHandler()}
												>
													{flexRender(header.column.columnDef.header, header.getContext())}
													{sorted === "asc" ? (
														<ArrowUp className="size-3" />
													) : sorted === "desc" ? (
														<ArrowDown className="size-3" />
													) : (
														<ChevronsUpDown className="size-3 opacity-50" />
													)}
												</button>
											) : (
												flexRender(header.column.columnDef.header, header.getContext())
											)}
										</TableHead>
									);
								})}
							</TableRow>
						))}
					</TableHeader>
					<TableBody>
						{rows.length === 0 ? (
							<TableRow>
								<TableCell colSpan={tableColumns.length} className="h-24 text-center">
									{empty ?? <span className="text-sm text-muted-foreground">暂无数据</span>}
								</TableCell>
							</TableRow>
						) : (
							rows.map((row) => (
								<TableRow
									key={row.id}
									data-state={row.getIsSelected() ? "selected" : undefined}
									className={cn(onRowClick && "cursor-pointer")}
									onClick={onRowClick ? () => onRowClick(row.original) : undefined}
								>
									{row.getVisibleCells().map((cell) => {
										const meta = cell.column.columnDef.meta as
											| { column?: DataTableColumn<T> }
											| undefined;
										const column = meta?.column;
										return (
											<TableCell
												key={cell.id}
												className={cn(
													column?.align ? alignClass[column.align] : "text-left",
													column?.hideBelow ? hideClass[column.hideBelow] : "",
													column?.className,
												)}
											>
												{flexRender(cell.column.columnDef.cell, cell.getContext())}
											</TableCell>
										);
									})}
								</TableRow>
							))
						)}
					</TableBody>
				</Table>
			</div>
		</div>
	);
}
