import type { QueryParams, UniResponse } from "@/api/axios";
import { diffToPatch } from "@/util";
import { KebabHorizontalIcon } from "@primer/octicons-react";
import {
    ActionList,
    ActionMenu,
    Button,
    ButtonGroup,
    Checkbox,
    ConfirmationDialog,
    FormControl,
    IconButton,
    Select,
    TextInput,
} from "@primer/react";
import {
    Banner,
    DataTable,
    Dialog,
    Table,
    type UniqueRow,
} from "@primer/react/experimental";
import {
    type UseQueryResult,
    keepPreviousData,
    useMutation,
    useQuery,
    useQueryClient,
} from "@tanstack/react-query";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";
import type { AxiosError } from "axios";
import {
    type ReactElement,
    type ReactNode,
    cloneElement,
    useCallback,
    useState,
} from "react";
import { FilterBar } from "./FilterBar";
import { type BannerVariant, useMsgBanner } from "./MsgBanner";
export type PaginationResponse<T> = {
    data: T[];
    meta: { total: number; page: number; limit: number };
};
export type Column<T> = {
    accessorKey: string; // 对应数据字段
    header: string | (() => ReactNode); // 表头，可自定义渲染
    id?: string; // 可选 id
    rowHeader?: boolean; // 用于标记行头
    renderCell?: (row: T) => ReactNode; // 自定义单元格渲染
    maxWidth?: string;
};
export type MutationColumn = {
    header: string;
    field: string;
    render: ReactElement;
    /** 仅在新增对话框展示（不可变身份字段等）。 */
    createOnly?: boolean;
    /** 仅在修改对话框展示。 */
    editOnly?: boolean;
};
export type BannerState = {
    isShown: boolean;
    description: string;
    variant: BannerVariant;
};
type RequireGetRowId<T> = T extends { id: string }
    ? // biome-ignore lint/complexity/noBannedTypes: <explanation>
      {}
    : { getRowId: (row: T) => string };

type GenericTableProps<T> = {
    subject: string; // 用作 queryKey
    columns: Column<T>[];
    queryFn: (params?: QueryParams) => Promise<UniResponse<T[]>>;
    createFn?: (data: Partial<T>) => Promise<UniResponse<T>>;
    removeFn?: (id_list: string[]) => Promise<UniResponse<number>>;
    patchFn?: (data: Partial<T>) => Promise<UniResponse<T>>;
    mutationColumns?: MutationColumn[];
    mutationData?: Partial<T>;
    /** 点 Add（新增）时重置表单到该默认值；未提供则保持现状（不重置）。 */
    defaultMutationData?: Partial<T>;
    customActions?: ReactNode;
    columnActions?: (row: T) => ReactNode;
    externalBanner?: ReturnType<typeof useMsgBanner>;
    enableInternalActions?: boolean;
    disableAdd?: boolean;
    hideTitle?: boolean;
    disablePagination?: boolean;
    disableSelect?: boolean;
    className?: string;
    subtitle?: string;
    getRowId?: (row: T) => string;
    selectedRowIds?: Set<string>;
    onSelectedRowIdsChange?: (ids: Set<string>) => void;
    filterKeys?: string[];
    /** 查询缓存新鲜时间，默认 30s；传 0 时每次挂载/翻页都重新请求（如 Instances 页）。 */
    staleTime?: number;
} & RequireGetRowId<T> &
    React.HTMLAttributes<HTMLDivElement>;

export const GenericTable = <T extends object>({
    subject,
    columns,
    queryFn,
    createFn,
    removeFn,
    patchFn,
    mutationColumns,
    mutationData,
    defaultMutationData,
    customActions,
    columnActions,
    externalBanner,
    enableInternalActions = true,
    disableAdd = false,
    disableSelect = false,
    staleTime = 30_000,
    hideTitle = false,
    disablePagination = false,
    subtitle,
    getRowId,
    selectedRowIds: externalSelectedRowIds,
    onSelectedRowIdsChange,
    filterKeys,
    ...rest
}: GenericTableProps<T>) => {
    const [internalSelectedRowIds, setInternalSelectedRowIds] = useState<
        Set<string>
    >(new Set());

    // 当前真正使用的状态 — 外部优先
    const selectedRowIds = externalSelectedRowIds ?? internalSelectedRowIds;

    const setSelectedRowIds = useCallback(
        (ids: Set<string>) => {
            if (onSelectedRowIdsChange) {
                onSelectedRowIdsChange(ids); // 父组件控制
            } else {
                setInternalSelectedRowIds(ids); // 内部自己控制
            }
        },
        [onSelectedRowIdsChange],
    );

    // 查询
    const [page, setPage] = useState(1);
    const [limit, setLimit] = useState(disablePagination ? 100 : 10);
    const [filter, setFilter] = useState("");
    const queryClient = useQueryClient();

    const { data, isLoading }: UseQueryResult<UniResponse<T[]>> = useQuery({
        queryKey: [subject, page, limit],
        queryFn: () => queryFn({ page, limit, filter }),
        // 列表数据缓存 30s，重复进入/翻页不重复请求；
        // 翻页时保留上一页数据占位，避免整表骨架屏闪烁。
        // staleTime 可覆盖（如 Instances 页传 0，切回标签必刷新）。
        staleTime,
        refetchOnWindowFocus: false,
        placeholderData: keepPreviousData,
    });
    // 向列追加操作
    const safeGetRowId = (row: T) => {
        function hasIdField(obj: unknown): obj is { id: string } {
            return typeof obj === "object" && obj !== null && "id" in obj;
        }
        if (getRowId) return getRowId(row);
        if (hasIdField(row)) return row.id;

        throw new Error(
            `GenericTable: 行数据没有 id 字段，请传 getRowId: ${JSON.stringify(row)}`,
        );
    };

    const tableColumns: Column<T>[] = (() => {
        if (disableSelect) return columns;

        const selectedColumn: Column<T> = {
            accessorKey: "selected",
            id: "selected",
            header: () => (
                <Checkbox
                    checked={
                        data?.data?.length
                            ? selectedRowIds.size === data.data.length
                            : false
                    }
                    onChange={(e) => {
                        if (e.target.checked) {
                            // 全选
                            setSelectedRowIds(
                                new Set(data?.data?.map(safeGetRowId) ?? []),
                            );
                        } else {
                            setSelectedRowIds(new Set());
                        }
                    }}
                />
            ),
            renderCell: (row: T) => {
                const rowId = safeGetRowId(row);
                return (
                    <Checkbox
                        checked={selectedRowIds.has(rowId)}
                        onChange={(e) => {
                            const newSet = new Set<string>(selectedRowIds);

                            if (e.target.checked) {
                                newSet.add(rowId);
                            } else {
                                newSet.delete(rowId);
                            }

                            setSelectedRowIds(newSet); // ✅ 只传 Set
                        }}
                    />
                );
            },
            maxWidth: "30px",
        };

        if (!enableInternalActions) {
            return [selectedColumn, ...columns];
        }

        // 没有 actions，添加默认 actions 列
        const actionsColumn: Column<T> = {
            accessorKey: "actions",
            id: "actions",
            header: "Actions",
            renderCell: (row: T) => (
                <ActionMenu>
                    <ActionMenu.Anchor>
                        <IconButton
                            aria-label={safeGetRowId(row)}
                            title={safeGetRowId(row)}
                            icon={KebabHorizontalIcon}
                            variant="invisible"
                        />
                    </ActionMenu.Anchor>
                    <ActionMenu.Overlay>
                        {columnActions?.(row)}
                        <ActionList.Divider />
                        <ActionList>
                            {columns.find(
                                (column) => column.accessorKey === "actions",
                            ) ? (
                                <></>
                            ) : (
                                <></>
                            )}
                            {patchFn && (
                                <ActionList.Item
                                    key={`${safeGetRowId(row)}-edit`}
                                    onClick={() => {
                                        setDialogMode("modify");
                                        setOriginalRow(row);
                                        setIsOpen(true);
                                        if (mutationData) {
                                            Object.assign(mutationData, row);
                                        }
                                    }}
                                >
                                    Edit row
                                </ActionList.Item>
                            )}

                            {removeFn && (
                                <>
                                    <ActionList.Divider />
                                    <ActionList.Item
                                        key={`${safeGetRowId(row)}-delete`}
                                        variant="danger"
                                        onClick={() => {
                                            deleteMutation?.mutate([
                                                safeGetRowId(row),
                                            ]);
                                            onDialogClose?.();
                                        }}
                                    >
                                        Delete row
                                    </ActionList.Item>
                                </>
                            )}
                        </ActionList>
                    </ActionMenu.Overlay>
                </ActionMenu>
            ),
        };

        return [selectedColumn, ...columns, actionsColumn];
    })();

    const total = data?.meta?.total ?? 1;
    const table = useReactTable({
        data: data?.data ?? [],
        columns: tableColumns,
        getCoreRowModel: getCoreRowModel(),
    });
    const [originalRow, setOriginalRow] = useState<Partial<T> | null>(null);
    const banner = externalBanner ?? useMsgBanner();

    // 新增或修改
    const [isOpen, setIsOpen] = useState(false);
    const [dialogMode, setDialogMode] = useState<"add" | "modify">("add");
    const onDialogClose = useCallback(() => setIsOpen(false), []);

    // 变更
    const deleteMutation = useMutation({
        mutationFn: removeFn,
        onSuccess: (_data, ids: string[]) => {
            // 乐观移除：删除成功后立即从所有 [subject, ...] 缓存里剔除对应行，
            // 列表马上消失；再 invalidate 拉取服务端真实状态兜底。
            // 之前只 invalidate 依赖后台 refetch，refetch 慢/失败时（React Query
            // 保留上次成功数据）已删行会残留到手动刷新，用户反馈过此类问题。
            const removed = new Set(ids);
            queryClient.setQueriesData(
                { queryKey: [subject] },
                (old: UniResponse<T[]> | undefined) => {
                    if (!old || !Array.isArray(old.data)) return old;
                    return {
                        ...old,
                        data: old.data.filter((row) => !removed.has(safeGetRowId(row))),
                    };
                },
            );
            queryClient.invalidateQueries({ queryKey: [subject] });
            banner.showBanner("success", `Delete ${subject} successfully`);
        },
        onError: (error) => {
            banner.showErrorBanner(error);
        },
    });

    const createMutation = useMutation({
        mutationFn: createFn,
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: [subject] });
            banner.showBanner("success", `Create ${subject} successfully`);
        },
        onError: (error) => {
            banner.showErrorBanner(error);
        },
    });

    const patchMutation = useMutation({
        mutationFn: patchFn,
        onSuccess: () => {
            queryClient.invalidateQueries({ queryKey: [subject] });
            banner.showBanner("success", `Update ${subject} successfully`);
        },
        onError: (error) => {
            banner.showErrorBanner(error);
        },
    });

    if (isLoading) {
        return (
            <Table.Skeleton
                aria-labelledby="repositories-loading"
                rows={limit}
                columns={tableColumns as Column<UniqueRow>[]}
            />
        );
    }

    return (
        <div className="w-full" {...rest}>
            {isOpen && (
                <Dialog
                    title={
                        dialogMode === "add"
                            ? `Add ${subject}`
                            : `Modify ${subject}`
                    }
                    onClose={onDialogClose}
                    position="right"
                >
                    <div className="w-full gap-1 flex-col flex">
                        {mutationColumns
                            ?.filter((column) => {
                                if (dialogMode === "add" && column.editOnly) {
                                    return false;
                                }
                                if (
                                    dialogMode === "modify" &&
                                    column.createOnly
                                ) {
                                    return false;
                                }
                                return true;
                            })
                            .map((column) => (
                            <FormControl key={column.field} className="w-full">
                                <FormControl.Label>
                                    {column.field}
                                </FormControl.Label>
                                {cloneElement(
                                    column.render as ReactElement<{
                                        className?: string;
                                    }>,
                                    { className: "w-full" },
                                )}
                            </FormControl>
                        ))}
                        {(dialogMode === "add" && (
                            <Button
                                className="w-full"
                                variant="primary"
                                onClick={() => {
                                    if (mutationData)
                                        createMutation.mutate(mutationData);
                                }}
                            >
                                Create
                            </Button>
                        )) ||
                            (dialogMode === "modify" && (
                                <div className="flex gap-1">
                                    <Button
                                        className="w-full"
                                        variant="primary"
                                        onClick={() => {
                                            if (mutationData && originalRow) {
                                                const payload = diffToPatch(
                                                    originalRow,
                                                    mutationData,
                                                );
                                                patchMutation.mutate(payload); // ✅ 只 PATCH 改动字段
                                            } else if (mutationData) {
                                                patchMutation.mutate(
                                                    mutationData,
                                                ); // fallback
                                            }
                                            setIsOpen(false);
                                        }}
                                    >
                                        Update
                                    </Button>
                                    <Button
                                        className="w-full"
                                        variant="danger"
                                        onClick={() => {
                                            if (mutationData)
                                                deleteMutation.mutate([
                                                    safeGetRowId(
                                                        mutationData as T,
                                                    ) as string,
                                                ]);
                                            setIsOpen(false);
                                        }}
                                    >
                                        Delete
                                    </Button>
                                </div>
                            ))}
                    </div>
                </Dialog>
            )}

            {/* table */}
            <Table.Container>
                {!hideTitle && (
                    <Table.Title id="repositories-headerAction">
                        {subject}
                    </Table.Title>
                )}

                <Table.Actions>
                    {selectedRowIds.size !== 0 && removeFn && (
                        <BulkDeleteButton
                            selectedRowIds={Array.from(selectedRowIds)}
                            setSelectedRowIds={setSelectedRowIds}
                            onConfirmDelete={(ids) =>
                                deleteMutation.mutate(ids)
                            }
                        />
                    )}

                    {!disableAdd && (
                        <Button
                            variant="primary"
                            onClick={() => {
                                if (mutationData) {
                                    // 新增：重置到 defaultMutationData（未提供则保持现状）。
                                    Object.assign(
                                        mutationData,
                                        defaultMutationData ?? {},
                                    );
                                }
                                setDialogMode("add");
                                setIsOpen(true);
                            }}
                        >
                            Add
                        </Button>
                    )}

                    {customActions}

                    {!disablePagination && (
                        <Select
                            value={String(limit)}
                            onChange={(e) => {
                                setLimit(Number(e.target.value));
                                setPage(1);
                            }}
                        >
                            <Select.Option value="10">10 / page</Select.Option>
                            <Select.Option value="20">20 / page</Select.Option>
                            <Select.Option value="50">50 / page</Select.Option>
                            <Select.Option value="100">
                                100 / page
                            </Select.Option>
                        </Select>
                    )}
                </Table.Actions>

                <Table.Divider />
                <Table.Subtitle id="repositories-subtitle-headerAction">
                    {subtitle && <p>{subtitle}</p>}
                    <banner.BannerComponent />
                    {filterKeys && (
                        <FilterBar
                            keys={filterKeys}
                            filter={filter}
                            setFilter={setFilter}
                            queryKey={subject}
                        />
                    )}
                </Table.Subtitle>

                <DataTable
                    aria-labelledby="repositories-default-headerAction"
                    aria-describedby="repositories-subtitle-headerAction"
                    data={
                        table
                            .getRowModel()
                            .rows.map(
                                (r) => r.original,
                            ) as unknown as UniqueRow[]
                    }
                    columns={tableColumns as Column<UniqueRow>[]}
                    getRowId={(row) => safeGetRowId(row as T)}
                />

                {!disablePagination && (
                    <Table.Pagination
                        aria-label="Pagination"
                        pageSize={limit}
                        totalCount={total}
                        defaultPageIndex={page - 1}
                        onChange={({ pageIndex }) => {
                            setPage(pageIndex + 1);
                        }}
                    />
                )}
            </Table.Container>
        </div>
    );
};

export const BulkDeleteButton = ({
    selectedRowIds,
    setSelectedRowIds,
    onConfirmDelete,
}: {
    selectedRowIds: string[];
    setSelectedRowIds: (ids: Set<string>) => void;
    onConfirmDelete: (ids: string[]) => void;
}) => {
    const [open, setOpen] = useState(false);

    if (selectedRowIds.length === 0) return null;

    return (
        <>
            <Button variant="danger" onClick={() => setOpen(true)}>
                Delete {selectedRowIds.length} selected
            </Button>

            {open && (
                <ConfirmationDialog
                    onClose={(gesture) => {
                        switch (gesture) {
                            case "confirm":
                                onConfirmDelete(selectedRowIds);
                                setSelectedRowIds(new Set());
                                break;
                        }
                        setOpen(false);
                    }}
                    title="Delete Challenges"
                    confirmButtonContent="Delete"
                    cancelButtonContent="Cancel"
                    confirmButtonType="danger"
                >
                    Are you sure you want to delete {selectedRowIds.length} item
                    {selectedRowIds.length > 1 ? "s" : ""}? This action cannot
                    be undone.
                </ConfirmationDialog>
            )}
        </>
    );
};
