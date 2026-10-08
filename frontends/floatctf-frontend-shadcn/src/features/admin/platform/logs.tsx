/**
 * 管理端 · 操作日志（`/admin/platform/logs`）。
 *
 * 真实接口：`client.admin.logs.fetch({ page, limit, filter })`。
 * 后端语义（`apps/api/src/modules/platform/operations/logs.rs`）：
 * - `filter` 支持精确/包含映射：`id`、`user_id`、`superadmin_id`、`ip_address`(contains)、
 *   `category`(eq)、`action`(contains)、`level`(eq)；
 * - `filter` 语法是 `key:value`，多个条件用 ` & ` 连接（`|` 是 OR 分组）；
 * - 实体里**没有** `target` 字段：日志维度是 `category` + `action`（以实体为准）。
 */

import { useEffect, useState, type ReactNode } from "react";
import {
	keepPreviousData,
	useQuery,
} from "@tanstack/react-query";
import { RefreshCw, ScrollText } from "lucide-react";

import type { Logs } from "@floatctf/sdk/entity";

import { callList } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill, type PillTone } from "~/components/app/badges";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import {
	CopyText,
	KeyValueList,
	MonoText,
	PageBody,
	PageHeader,
	PaginationBar,
	ReadonlyBlock,
	SectionCard,
	Toolbar,
} from "~/components/app/page";
import {
	EmptyBlock,
	QueryState,
	RefreshingBadge,
	TableSkeleton,
} from "~/components/app/states";
import { AbsoluteTime } from "~/components/app/user-cell";
import { Button } from "~/components/ui/button";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "~/components/ui/select";
import {
	Sheet,
	SheetContent,
	SheetDescription,
	SheetHeader,
	SheetTitle,
} from "~/components/ui/sheet";
import { truncate } from "~/lib/format";
import { useDocumentTitle } from "~/lib/hooks";

import { SearchInput, jsonText, sanitizeFilterValue, useListState } from "./components";

const PAGE_SIZE_OPTIONS = [20, 50, 100];

/** 日志级别（后端 `add_log` 使用大写：INFO / WARN / ERROR / DEBUG）。 */
const LEVELS = ["INFO", "WARN", "ERROR", "DEBUG"] as const;

const LEVEL_TONE: Record<string, PillTone> = {
	info: "info",
	warn: "warning",
	warning: "warning",
	error: "danger",
	debug: "muted",
};

type SearchField = "action" | "category" | "ip_address" | "user_id" | "superadmin_id";

const SEARCH_FIELDS: Array<{ value: SearchField; label: string; hint: string }> = [
	{ value: "action", label: "操作", hint: "包含匹配" },
	{ value: "category", label: "分类", hint: "精确匹配" },
	{ value: "ip_address", label: "IP", hint: "包含匹配" },
	{ value: "user_id", label: "用户 ID", hint: "精确匹配（UUID）" },
	{ value: "superadmin_id", label: "管理员 ID", hint: "精确匹配（UUID）" },
];

function levelTone(level: string): PillTone {
	return LEVEL_TONE[level.toLowerCase()] ?? "neutral";
}

export function AdminLogsPage(): ReactNode {
	useDocumentTitle("操作日志 · FloatCTF 控制台");
	const client = useClient();
	const list = useListState(50);
	const [level, setLevel] = useState<string>("all");
	const [field, setField] = useState<SearchField>("action");
	const [openLog, setOpenLog] = useState<Logs | null>(null);

	const searchValue = sanitizeFilterValue(list.filter);
	const conditions: string[] = [];
	if (level !== "all") conditions.push(`level:${level}`);
	if (searchValue.length > 0) conditions.push(`${field}:${searchValue}`);
	const remoteFilter = conditions.length > 0 ? conditions.join(" & ") : undefined;

	useEffect(() => {
		list.setPage(1);
		// 级别 / 字段变化也需要回到第 1 页（list.filter 变化由 useListState 处理）。
	}, [level, field, list.setPage]);

	const params = {
		page: list.page,
		limit: list.pageSize,
		...(remoteFilter ? { filter: remoteFilter } : {}),
	};

	const query = useQuery({
		queryKey: qk.admin.logs(params),
		queryFn: () => callList<Logs>(client.admin.logs.fetch(params)),
		placeholderData: keepPreviousData,
	});

	const fieldMeta = SEARCH_FIELDS.find((item) => item.value === field);

	const columns: DataTableColumn<Logs>[] = [
		{
			id: "created_at",
			header: "时间",
			sortValue: (row) => Date.parse(row.created_at),
			cell: (row) => <AbsoluteTime value={row.created_at} seconds />,
		},
		{
			id: "level",
			header: "级别",
			sortValue: (row) => row.level,
			cell: (row) => <TonePill tone={levelTone(row.level)}>{row.level}</TonePill>,
		},
		{
			id: "category",
			header: "分类",
			hideBelow: "sm",
			sortValue: (row) => row.category,
			cell: (row) => <MonoText>{row.category}</MonoText>,
		},
		{
			id: "action",
			header: "操作",
			sortValue: (row) => row.action,
			cell: (row) => <MonoText>{row.action}</MonoText>,
		},
		{
			id: "message",
			header: "消息",
			cell: (row) => (
				<span className="break-words" title={row.message}>
					{truncate(row.message, 80)}
				</span>
			),
		},
		{
			id: "ip_address",
			header: "IP",
			align: "right",
			hideBelow: "lg",
			sortValue: (row) => row.ip_address ?? "",
			cell: (row) => <MonoText className="text-muted-foreground">{row.ip_address ?? "—"}</MonoText>,
		},
	];

	return (
		<PageBody>
			<PageHeader
				title="操作日志"
				description="平台审计日志（只读）。级别为精确匹配，其余字段按后端映射做包含 / 精确匹配。"
				actions={
					<Toolbar>
						<RefreshingBadge active={query.isFetching && !query.isPending} />
						<Button
							variant="outline"
							size="sm"
							onClick={() => void query.refetch()}
							disabled={query.isFetching}
						>
							<RefreshCw />
							刷新
						</Button>
					</Toolbar>
				}
			/>

			<SectionCard
				title="日志列表"
				description="按创建时间倒序；点行查看完整消息与 details（JSON）。"
				actions={
					<div className="flex flex-wrap items-center gap-2">
						<Select
							value={field}
							onValueChange={(value) => setField(value as SearchField)}
						>
							<SelectTrigger size="sm" className="w-[132px]" aria-label="搜索字段">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{SEARCH_FIELDS.map((item) => (
									<SelectItem key={item.value} value={item.value}>
										{item.label}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
						<Select value={level} onValueChange={setLevel}>
							<SelectTrigger size="sm" className="w-[130px]" aria-label="日志级别">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value="all">全部级别</SelectItem>
								{LEVELS.map((item) => (
									<SelectItem key={item} value={item}>
										{item}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
						<SearchInput
							value={list.search}
							onChange={list.setSearch}
							placeholder={`搜索${fieldMeta?.label ?? "日志"}（${fieldMeta?.hint ?? ""}）`}
						/>
					</div>
				}
			>
				<QueryState
					query={query}
					skeleton={<TableSkeleton rows={8} columns={6} />}
					errorTitle="加载日志失败"
					isEmpty={(result) => result.items.length === 0}
					empty={
						<EmptyBlock
							variant={remoteFilter ? "filtered" : "empty"}
							title={remoteFilter ? "没有匹配的日志" : "暂无日志"}
							description={
								remoteFilter
									? "试试调整级别或搜索条件（注意级别为精确匹配）。"
									: "平台产生审计记录后会出现在这里。"
							}
						/>
					}
				>
					{(result) => (
						<div className="space-y-3">
							<DataTable
								data={result.items}
								getRowId={(row) => row.id}
								columns={columns}
								onRowClick={(row) => setOpenLog(row)}
								mobileCard={(row) => (
									<button
										type="button"
										className="w-full space-y-2 text-left"
										onClick={() => setOpenLog(row)}
									>
										<div className="flex items-center justify-between gap-2">
											<TonePill tone={levelTone(row.level)}>{row.level}</TonePill>
											<AbsoluteTime value={row.created_at} seconds />
										</div>
										<p className="text-sm break-words">{row.message}</p>
										<p className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
											<MonoText>{row.category}</MonoText>
											<MonoText>{row.action}</MonoText>
											<MonoText>{row.ip_address ?? "—"}</MonoText>
										</p>
									</button>
								)}
							/>
							<PaginationBar
								page={list.page}
								pageSize={list.pageSize}
								total={result.meta.total ?? result.items.length}
								onPageChange={list.setPage}
								onPageSizeChange={list.setPageSize}
								pageSizeOptions={PAGE_SIZE_OPTIONS}
							/>
						</div>
					)}
				</QueryState>
			</SectionCard>

			<Sheet open={openLog !== null} onOpenChange={(open) => !open && setOpenLog(null)}>
				<SheetContent className="flex w-full flex-col gap-0 overflow-y-auto sm:max-w-2xl">
					<SheetHeader>
						<SheetTitle className="flex items-center gap-2">
							<ScrollText className="size-4" />
							日志详情
						</SheetTitle>
						<SheetDescription>
							{openLog ? <CopyText value={openLog.id} label="日志 ID" /> : null}
						</SheetDescription>
					</SheetHeader>
					{openLog ? (
						<div className="space-y-4 px-4 pb-6">
							<KeyValueList
								items={[
									{
										key: "时间",
										value: <AbsoluteTime value={openLog.created_at} seconds />,
									},
									{
										key: "级别",
										value: <TonePill tone={levelTone(openLog.level)}>{openLog.level}</TonePill>,
									},
									{ key: "分类", value: <MonoText>{openLog.category}</MonoText> },
									{ key: "操作", value: <MonoText>{openLog.action}</MonoText> },
									{
										key: "来源 IP",
										value: <MonoText>{openLog.ip_address ?? "—"}</MonoText>,
									},
									{
										key: "用户 ID",
										value: <MonoText>{openLog.user_id ?? "—"}</MonoText>,
									},
									{
										key: "管理员 ID",
										value: <MonoText>{openLog.superadmin_id ?? "—"}</MonoText>,
									},
								]}
								columns={2}
							/>
							<div className="space-y-1">
								<p className="text-xs text-muted-foreground">消息</p>
								<ReadonlyBlock>{openLog.message}</ReadonlyBlock>
							</div>
							<div className="space-y-1">
								<p className="text-xs text-muted-foreground">详情（JSON）</p>
								<ReadonlyBlock>
									<pre className="max-h-96 overflow-auto font-mono text-xs">
										{jsonText(openLog.details) || "（空）"}
									</pre>
								</ReadonlyBlock>
							</div>
						</div>
					) : null}
				</SheetContent>
			</Sheet>
		</PageBody>
	);
}
