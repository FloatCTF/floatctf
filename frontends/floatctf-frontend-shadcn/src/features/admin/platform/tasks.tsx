/**
 * 管理端 · 计划任务（`/admin/platform/tasks`）。
 *
 * 真实接口：`client.admin.scheduled_tasks.fetch/create/patch/remove/run`。
 * 后端语义（`apps/api/src/modules/platform/operations/scheduled_tasks.rs`）：
 * - 列表支持 `page`/`limit`，`filter` 映射 `id`/`task_name`/`task_key`/`trigger_type`/
 *   `status`/`enabled`/`protected` 以及分组条件 `kind:service|system|event`；
 * - **`run` 不是立即执行**：它把 `status` 置为 `pending`、`execute_at` 置为当前时间并唤醒
 *   调度器（毫秒级入队）—— 文案必须如实反映；
 * - `create`/`patch` 都会校验 `trigger_type`：`once` 必须给 `execute_at`，
 *   `cron` 必须给合法 Cron 表达式，`task_key` 必须是平台已注册的任务键；
 * - `protected=true` 的任务后端**禁止删除**（400）；
 * - `create`/`patch` 的 DTO **不接受** `max_attempts` / `timeout_secs`，
 *   它们是运行时字段（只读展示）；`payload` 在后端是 `serde_json::Value`，但 SDK
 *   把实体字段声明成 `string`（见报告中的 SDK GAP），因此本页不提供 payload 编辑。
 */

import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import {
	keepPreviousData,
	useMutation,
	useQuery,
	useQueryClient,
} from "@tanstack/react-query";
import {
	MoreHorizontal,
	Pencil,
	Play,
	Plus,
	RefreshCw,
	ScrollText,
	Trash2,
} from "lucide-react";

import type { ScheduledTasks } from "@floatctf/sdk/entity";

import { call, callList, callVoid } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { BooleanPill, TonePill, type PillTone } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { Field, FormFooter, FormGrid, FormSheet } from "~/components/app/form";
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
import { toast } from "~/components/app/toast";
import { AbsoluteTime } from "~/components/app/user-cell";
import { Button } from "~/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "~/components/ui/dropdown-menu";
import { Input } from "~/components/ui/input";
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
import { Switch } from "~/components/ui/switch";
import { Textarea } from "~/components/ui/textarea";
import { fromDatetimeLocalValue, toDatetimeLocalValue, truncate } from "~/lib/format";
import { useDocumentTitle } from "~/lib/hooks";

import {
	DeleteSelectedButton,
	SearchInput,
	jsonText,
	sanitizeFilterValue,
	textOf,
	useListState,
} from "./components";

const PAGE_SIZE_OPTIONS = [10, 20, 50];
const DELETE_PHRASE = "DELETE";

type Kind = "all" | "service" | "system" | "event";
type SearchField = "task_name" | "task_key" | "status";

const SEARCH_FIELDS: Array<{ value: SearchField; label: string }> = [
	{ value: "task_name", label: "任务名称" },
	{ value: "task_key", label: "任务键" },
	{ value: "status", label: "状态" },
];

const KINDS: Array<{ value: Kind; label: string }> = [
	{ value: "all", label: "全部分组" },
	{ value: "service", label: "ServiceTasks（管理员自建）" },
	{ value: "system", label: "SystemTask（平台内置）" },
	{ value: "event", label: "EventTasks（赛事运行时）" },
];

const TRIGGER_TYPES = ["once", "cron", "startup"] as const;

function statusTone(status: string): PillTone {
	switch (status) {
		case "pending":
			return "info";
		case "running":
			return "warning";
		case "completed":
			return "success";
		case "failed":
			return "danger";
		default:
			return "neutral";
	}
}

export function AdminScheduledTasksPage(): ReactNode {
	useDocumentTitle("计划任务 · FloatCTF 控制台");
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const list = useListState(20);
	const [kind, setKind] = useState<Kind>("all");
	const [field, setField] = useState<SearchField>("task_name");
	const [selected, setSelected] = useState<string[]>([]);
	const [editing, setEditing] = useState<ScheduledTasks | null | undefined>(undefined);
	const [openTask, setOpenTask] = useState<ScheduledTasks | null>(null);

	const searchValue = sanitizeFilterValue(list.filter);
	const conditions: string[] = [];
	if (kind !== "all") conditions.push(`kind:${kind}`);
	if (searchValue.length > 0) conditions.push(`${field}:${searchValue}`);
	const remoteFilter = conditions.length > 0 ? conditions.join(" & ") : undefined;

	useEffect(() => {
		list.setPage(1);
	}, [kind, field, list.setPage]);

	useEffect(() => {
		setSelected([]);
	}, [list.page, remoteFilter]);

	const params = {
		page: list.page,
		limit: list.pageSize,
		...(remoteFilter ? { filter: remoteFilter } : {}),
	};

	const query = useQuery({
		queryKey: qk.admin.scheduledTasks(params),
		queryFn: () => callList<ScheduledTasks>(client.admin.scheduled_tasks.fetch(params)),
		placeholderData: keepPreviousData,
	});

	const runMutation = useMutation({
		mutationFn: (task: ScheduledTasks) =>
			call(client.admin.scheduled_tasks.run(task.id), "手动执行任务"),
		onSuccess: (task) => {
			toast.success(
				`任务「${task.task_name}」已入队`,
				`状态 ${task.status}；调度器会在毫秒级拉起执行，稍后刷新查看结果。`,
			);
			void queryClient.invalidateQueries({ queryKey: qk.admin.scheduledTasks() });
		},
		onError: (error) => toast.apiError("手动执行失败", error),
	});

	const removeMutation = useMutation({
		mutationFn: (ids: string[]) =>
			callVoid(client.admin.scheduled_tasks.remove(ids), "删除计划任务"),
		onSuccess: (_data, ids) => {
			toast.success(`已删除 ${ids.length} 条计划任务`);
			setSelected([]);
			void queryClient.invalidateQueries({ queryKey: qk.admin.scheduledTasks() });
		},
		onError: (error) => toast.apiError("删除计划任务失败", error),
	});

	async function runTask(task: ScheduledTasks) {
		const ok = await confirm({
			title: `立即执行「${task.task_name}」？`,
			description: `任务键 ${task.task_key} · 触发方式 ${task.trigger_type}`,
			consequences: [
				"调度器会立即拉起该任务（status 置为 pending、execute_at 置为当前时间）",
				"任务会真实修改平台数据 / 调用宿主 helper，其副作用与定时触发完全一致",
				"重复点击会重复入队；请先确认任务当前没有正在执行",
			],
			tone: "danger",
			confirmText: "入队执行",
		});
		if (!ok) return;
		runMutation.mutate(task);
	}

	async function removeRows(rows: ScheduledTasks[]) {
		const deletable = rows.filter((row) => !row.protected);
		const blocked = rows.filter((row) => row.protected);
		if (blocked.length > 0) {
			toast.warning(
				"受保护任务无法删除",
				`已跳过：${blocked.map((row) => row.task_name).join("、")}（后端会拒绝删除 protected 任务）`,
			);
		}
		if (deletable.length === 0) return;
		const ok = await confirm({
			title:
				deletable.length === 1
					? `删除任务「${deletable[0].task_name}」？`
					: `删除 ${deletable.length} 条计划任务？`,
			description: deletable.map((row) => row.task_name).join("、"),
			consequences: [
				"任务不再被调度：正在等待的 execute_at / cron 触发会失效",
				"若任务已被其它业务引用，相关自动化流程会静默停止",
				"删除后不可恢复（需要重新创建并填回任务键与触发参数）",
			],
			tone: "danger",
			confirmText: "永久删除",
			confirmPhrase: DELETE_PHRASE,
		});
		if (!ok) return;
		removeMutation.mutate(deletable.map((row) => row.id));
	}

	const columns: DataTableColumn<ScheduledTasks>[] = [
		{
			id: "task_name",
			header: "任务名称",
			sortValue: (row) => row.task_name,
			cell: (row) => (
				<span className="inline-flex items-center gap-1.5">
					<span className="font-medium">{row.task_name}</span>
					{row.protected ? <TonePill tone="muted">受保护</TonePill> : null}
				</span>
			),
		},
		{
			id: "task_key",
			header: "任务键",
			hideBelow: "sm",
			sortValue: (row) => row.task_key,
			cell: (row) => <MonoText>{row.task_key}</MonoText>,
		},
		{
			id: "trigger_type",
			header: "触发",
			hideBelow: "md",
			sortValue: (row) => row.trigger_type,
			cell: (row) => <TonePill tone="neutral">{row.trigger_type}</TonePill>,
		},
		{
			id: "status",
			header: "状态",
			sortValue: (row) => row.status,
			cell: (row) => <TonePill tone={statusTone(row.status)}>{row.status}</TonePill>,
		},
		{
			id: "schedule",
			header: "计划",
			hideBelow: "lg",
			cell: (row) =>
				row.cron_expr ? (
					<MonoText className="text-muted-foreground">{row.cron_expr}</MonoText>
				) : row.execute_at ? (
					<AbsoluteTime value={row.execute_at} seconds />
				) : (
					<span className="text-muted-foreground">—</span>
				),
		},
		{
			id: "enabled",
			header: "启用",
			hideBelow: "md",
			cell: (row) => <BooleanPill value={row.enabled} trueText="启用" falseText="停用" />,
		},
		{
			id: "last_run_at",
			header: "上次执行",
			align: "right",
			hideBelow: "xl",
			sortValue: (row) => Date.parse(row.last_run_at ?? ""),
			cell: (row) =>
				row.last_run_at ? <AbsoluteTime value={row.last_run_at} /> : <span>—</span>,
		},
		{
			id: "updated_at",
			header: "更新时间",
			align: "right",
			sortValue: (row) => Date.parse(row.updated_at),
			cell: (row) => <AbsoluteTime value={row.updated_at} />,
		},
	];

	return (
		<PageBody>
			<PageHeader
				title="计划任务"
				description="平台定时 / 一次性 / 启动任务。手动执行只会把任务入队，不会同步等待结果。"
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
						<Button size="sm" onClick={() => setEditing(null)}>
							<Plus />
							新建任务
						</Button>
					</Toolbar>
				}
			/>

			<SectionCard
				title="任务列表"
				description="搜索与分组条件会同时下发给后端（filter 语法：`kind:x & task_key:y`）。"
				actions={
					<div className="flex flex-wrap items-center gap-2">
						<Select value={kind} onValueChange={(value) => setKind(value as Kind)}>
							<SelectTrigger size="sm" className="w-[190px]" aria-label="任务分组">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{KINDS.map((item) => (
									<SelectItem key={item.value} value={item.value}>
										{item.label}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
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
						<SearchInput
							value={list.search}
							onChange={list.setSearch}
							placeholder={`搜索${SEARCH_FIELDS.find((item) => item.value === field)?.label ?? ""}`}
						/>
						<DeleteSelectedButton
							count={selected.length}
							isPending={removeMutation.isPending}
							label="删除所选任务"
							onClick={() =>
								void removeRows(
									(query.data?.items ?? []).filter((row) => selected.includes(row.id)),
								)
							}
						/>
					</div>
				}
			>
				<QueryState
					query={query}
					skeleton={<TableSkeleton rows={8} columns={6} />}
					errorTitle="加载计划任务失败"
					isEmpty={(result) => result.items.length === 0}
					empty={
						<EmptyBlock
							variant={remoteFilter ? "filtered" : "empty"}
							title={remoteFilter ? "没有匹配的任务" : "暂无计划任务"}
							description={
								remoteFilter
									? "试试切换分组或调整搜索条件。"
									: "平台内置任务由后端补种；管理员也可以新建一次性 / Cron 任务。"
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
								selectable
								selectedIds={selected}
								onSelectionChange={setSelected}
								onRowClick={(row) => setOpenTask(row)}
								rowActions={(row) => (
									<DropdownMenu>
										<DropdownMenuTrigger asChild>
											<Button variant="ghost" size="icon-sm" aria-label="任务操作">
												<MoreHorizontal />
											</Button>
										</DropdownMenuTrigger>
										<DropdownMenuContent align="end">
											<DropdownMenuItem
												disabled={runMutation.isPending}
												onSelect={() => void runTask(row)}
											>
												<Play />
												立即执行
											</DropdownMenuItem>
											<DropdownMenuItem onSelect={() => setEditing(row)}>
												<Pencil />
												编辑
											</DropdownMenuItem>
											<DropdownMenuItem onSelect={() => setOpenTask(row)}>
												<ScrollText />
												运行详情
											</DropdownMenuItem>
											<DropdownMenuItem
												variant="destructive"
												disabled={row.protected}
												onSelect={() => void removeRows([row])}
											>
												<Trash2 />
												{row.protected ? "受保护，不可删除" : "删除"}
											</DropdownMenuItem>
										</DropdownMenuContent>
									</DropdownMenu>
								)}
								mobileCard={(row) => (
									<div className="space-y-2">
										<div className="flex items-start justify-between gap-2">
											<span className="font-medium">{row.task_name}</span>
											<TonePill tone={statusTone(row.status)}>{row.status}</TonePill>
										</div>
										<p className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
											<MonoText>{row.task_key}</MonoText>
											<TonePill tone="muted">{row.trigger_type}</TonePill>
											<BooleanPill value={row.enabled} trueText="启用" falseText="停用" />
										</p>
										{row.error_msg || row.last_error ? (
											<p className="text-xs text-destructive">
												{truncate(row.error_msg ?? row.last_error ?? "", 90)}
											</p>
										) : null}
										<div className="flex flex-wrap items-center gap-2">
											<Button
												variant="ghost"
												size="sm"
												disabled={runMutation.isPending}
												onClick={() => void runTask(row)}
											>
												<Play />
												立即执行
											</Button>
											<Button variant="ghost" size="sm" onClick={() => setEditing(row)}>
												<Pencil />
												编辑
											</Button>
											<Button
												variant="ghost"
												size="sm"
												className="text-destructive"
												disabled={row.protected}
												onClick={() => void removeRows([row])}
											>
												<Trash2 />
												删除
											</Button>
										</div>
									</div>
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

			<ScheduledTaskFormSheet
				open={editing !== undefined}
				row={editing ?? null}
				onOpenChange={(open) => {
					if (!open) setEditing(undefined);
				}}
			/>

			<Sheet open={openTask !== null} onOpenChange={(open) => !open && setOpenTask(null)}>
				<SheetContent className="flex w-full flex-col gap-0 overflow-y-auto sm:max-w-2xl">
					<SheetHeader>
						<SheetTitle>{openTask ? openTask.task_name : "运行详情"}</SheetTitle>
						<SheetDescription>
							{openTask ? <CopyText value={openTask.id} label="任务 ID" /> : null}
						</SheetDescription>
					</SheetHeader>
					{openTask ? (
						<div className="space-y-4 px-4 pb-6">
							<KeyValueList
								columns={2}
								items={[
									{ key: "任务键", value: <MonoText>{openTask.task_key}</MonoText> },
									{
										key: "状态",
										value: (
											<TonePill tone={statusTone(openTask.status)}>{openTask.status}</TonePill>
										),
									},
									{
										key: "触发方式",
										value: <MonoText>{openTask.trigger_type}</MonoText>,
									},
									{
										key: "Cron",
										value: <MonoText>{openTask.cron_expr ?? "—"}</MonoText>,
									},
									{
										key: "执行时间",
										value: openTask.execute_at ? (
											<AbsoluteTime value={openTask.execute_at} seconds />
										) : (
											<span>—</span>
										),
									},
									{
										key: "过期时间",
										value: openTask.expires_at ? (
											<AbsoluteTime value={openTask.expires_at} seconds />
										) : (
											<span>—</span>
										),
									},
									{
										key: "分组 ID",
										value: <MonoText>{openTask.group_id ?? "—"}</MonoText>,
										hint: "非空表示赛事运行时任务",
									},
									{
										key: "尝试次数",
										value: (
											<MonoText>
												{textOf(openTask.attempt_count)} / {textOf(openTask.max_attempts)}
											</MonoText>
										),
									},
									{
										key: "超时",
										value: <MonoText>{textOf(openTask.timeout_secs)}</MonoText>,
										hint: "运行时字段，创建/编辑接口不接受",
									},
									{
										key: "上次执行",
										value: openTask.last_run_at ? (
											<AbsoluteTime value={openTask.last_run_at} seconds />
										) : (
											<span>—</span>
										),
									},
									{
										key: "锁定时间",
										value: openTask.locked_at ? (
											<AbsoluteTime value={openTask.locked_at} seconds />
										) : (
											<span>—</span>
										),
									},
									{
										key: "心跳",
										value: openTask.heartbeat_at ? (
											<AbsoluteTime value={openTask.heartbeat_at} seconds />
										) : (
											<span>—</span>
										),
									},
								]}
							/>
							<div className="space-y-1">
								<p className="text-xs text-muted-foreground">描述</p>
								<ReadonlyBlock>{openTask.description ?? ""}</ReadonlyBlock>
							</div>
							{openTask.error_msg || openTask.last_error ? (
								<div className="space-y-1">
									<p className="text-xs text-destructive">最近错误</p>
									<ReadonlyBlock className="border-destructive/30 bg-destructive/5 font-mono text-xs">
										{openTask.error_msg ?? openTask.last_error ?? ""}
									</ReadonlyBlock>
								</div>
							) : null}
							<div className="space-y-1">
								<p className="text-xs text-muted-foreground">
									payload（JSON，只读：SDK 把它声明成 string，见报告 SDK GAP）
								</p>
								<ReadonlyBlock>
									<pre className="max-h-72 overflow-auto font-mono text-xs">
										{jsonText(openTask.payload) || "（空）"}
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

function ScheduledTaskFormSheet({
	open,
	row,
	onOpenChange,
}: {
	open: boolean;
	row: ScheduledTasks | null;
	onOpenChange: (open: boolean) => void;
}): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const [form, setForm] = useState({
		taskName: "",
		taskKey: "",
		triggerType: "once" as (typeof TRIGGER_TYPES)[number],
		cronExpr: "",
		executeAt: "",
		expiresAt: "",
		description: "",
		enabled: true,
		protected: false,
	});
	const [errors, setErrors] = useState<{
		taskName?: string;
		taskKey?: string;
		executeAt?: string;
		cronExpr?: string;
	}>({});

	useEffect(() => {
		if (!open) return;
		setErrors({});
		setForm({
			taskName: row?.task_name ?? "",
			taskKey: row?.task_key ?? "",
			triggerType: (row?.trigger_type as (typeof TRIGGER_TYPES)[number]) ?? "once",
			cronExpr: row?.cron_expr ?? "",
			executeAt: row?.execute_at ? toDatetimeLocalValue(row.execute_at) : "",
			expiresAt: row?.expires_at ? toDatetimeLocalValue(row.expires_at) : "",
			description: row?.description ?? "",
			enabled: row?.enabled ?? true,
			protected: row?.protected ?? false,
		});
	}, [open, row]);

	const createMutation = useMutation({
		mutationFn: (input: Partial<ScheduledTasks>) =>
			call(client.admin.scheduled_tasks.create(input), "创建计划任务"),
		onSuccess: () => {
			toast.success("计划任务已创建");
			void queryClient.invalidateQueries({ queryKey: qk.admin.scheduledTasks() });
			onOpenChange(false);
		},
		onError: (error) => toast.apiError("创建计划任务失败", error),
	});

	const patchMutation = useMutation({
		mutationFn: (input: Partial<ScheduledTasks>) =>
			call(client.admin.scheduled_tasks.patch(input), "更新计划任务"),
		onSuccess: () => {
			toast.success("计划任务已更新");
			void queryClient.invalidateQueries({ queryKey: qk.admin.scheduledTasks() });
			onOpenChange(false);
		},
		onError: (error) => toast.apiError("更新计划任务失败", error),
	});

	const isPending = createMutation.isPending || patchMutation.isPending;

	function submit(event: FormEvent<HTMLFormElement>) {
		event.preventDefault();
		const taskName = form.taskName.trim();
		const taskKey = form.taskKey.trim();
		const nextErrors: typeof errors = {};
		if (taskName.length === 0) nextErrors.taskName = "任务名称不能为空";
		if (taskKey.length === 0) nextErrors.taskKey = "任务键不能为空";
		if (form.triggerType === "once" && form.executeAt.length === 0) {
			nextErrors.executeAt = "触发方式 once 必须填写执行时间（后端同样会拒绝）";
		}
		if (form.triggerType === "cron" && form.cronExpr.trim().length === 0) {
			nextErrors.cronExpr = "触发方式 cron 必须填写 Cron 表达式（后端同样会拒绝）";
		}
		setErrors(nextErrors);
		if (Object.keys(nextErrors).length > 0) return;

		const executeAt = fromDatetimeLocalValue(form.executeAt);
		const expiresAt = fromDatetimeLocalValue(form.expiresAt);
		const payload: Partial<ScheduledTasks> = {
			task_name: taskName,
			task_key: taskKey,
			trigger_type: form.triggerType,
			description: form.description,
			enabled: form.enabled,
			protected: form.protected,
			...(form.cronExpr.trim().length > 0 ? { cron_expr: form.cronExpr.trim() } : {}),
			...(executeAt ? { execute_at: executeAt } : {}),
			...(expiresAt ? { expires_at: expiresAt } : {}),
		};
		if (row) patchMutation.mutate({ ...payload, id: row.id });
		else createMutation.mutate(payload);
	}

	return (
		<FormSheet
			open={open}
			onOpenChange={onOpenChange}
			title={row ? `编辑任务 ${row.task_name}` : "新建计划任务"}
			description="任务键必须是平台已注册的键（后端校验），未知键会被拒绝并返回具体错误。"
			width="lg"
			footer={
				<FormFooter
					formId="scheduled-task-form"
					isPending={isPending}
					submitLabel={row ? "保存" : "创建"}
					onCancel={() => onOpenChange(false)}
					hint={row ? `ID ${row.id}` : "任务名称、任务键与触发参数由后端校验"}
				/>
			}
		>
			<form id="scheduled-task-form" className="space-y-4" onSubmit={submit}>
				<FormGrid columns={2}>
					<Field label="任务名称" htmlFor="task-name" required error={errors.taskName}>
						<Input
							id="task-name"
							value={form.taskName}
							onChange={(event) => setForm({ ...form, taskName: event.target.value })}
						/>
					</Field>
					<Field
						label="任务键"
						htmlFor="task-key"
						required
						error={errors.taskKey}
						hint="平台注册的 TaskKey，例如 awd.round.end；未知键后端返回 400。"
					>
						<Input
							id="task-key"
							className="font-mono text-xs"
							value={form.taskKey}
							onChange={(event) => setForm({ ...form, taskKey: event.target.value })}
						/>
					</Field>
				</FormGrid>
				<FormGrid columns={2}>
					<Field label="触发方式" htmlFor="task-trigger">
						<Select
							value={form.triggerType}
							onValueChange={(value) =>
								setForm({ ...form, triggerType: value as (typeof TRIGGER_TYPES)[number] })
							}
						>
							<SelectTrigger id="task-trigger" className="w-full">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{TRIGGER_TYPES.map((type) => (
									<SelectItem key={type} value={type}>
										{type}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</Field>
					<Field
						label="Cron 表达式"
						htmlFor="task-cron"
						error={errors.cronExpr}
						hint="trigger_type=cron 时必填（后端用 cron 库校验）。"
					>
						<Input
							id="task-cron"
							className="font-mono text-xs"
							placeholder="0 */5 * * * * *"
							value={form.cronExpr}
							onChange={(event) => setForm({ ...form, cronExpr: event.target.value })}
						/>
					</Field>
				</FormGrid>
				<FormGrid columns={2}>
					<Field
						label="执行时间"
						htmlFor="task-execute-at"
						error={errors.executeAt}
						hint="trigger_type=once 时必填；留空表示不修改。"
					>
						<Input
							id="task-execute-at"
							type="datetime-local"
							value={form.executeAt}
							onChange={(event) => setForm({ ...form, executeAt: event.target.value })}
						/>
					</Field>
					<Field label="过期时间" htmlFor="task-expires-at" hint="留空表示不过期。">
						<Input
							id="task-expires-at"
							type="datetime-local"
							value={form.expiresAt}
							onChange={(event) => setForm({ ...form, expiresAt: event.target.value })}
						/>
					</Field>
				</FormGrid>
				<Field label="描述" htmlFor="task-description">
					<Textarea
						id="task-description"
						style={{ minHeight: 80 }}
						value={form.description}
						onChange={(event) => setForm({ ...form, description: event.target.value })}
					/>
				</Field>
				<FormGrid columns={2}>
					<Field label="启用" htmlFor="task-enabled" hint="停用后调度器不会拉起该任务。">
						<div className="flex items-center gap-2">
							<Switch
								id="task-enabled"
								checked={form.enabled}
								onCheckedChange={(checked) => setForm({ ...form, enabled: checked })}
							/>
							<BooleanPill value={form.enabled} trueText="启用" falseText="停用" />
						</div>
					</Field>
					<Field
						label="受保护"
						htmlFor="task-protected"
						hint="受保护任务不能被删除（后端拒绝），请谨慎开启。"
					>
						<div className="flex items-center gap-2">
							<Switch
								id="task-protected"
								checked={form.protected}
								onCheckedChange={(checked) => setForm({ ...form, protected: checked })}
							/>
							<BooleanPill value={form.protected} trueText="受保护" falseText="可删除" />
						</div>
					</Field>
				</FormGrid>
				<p className="text-xs text-muted-foreground">
					说明：`max_attempts` / `timeout_secs` 是运行时字段，创建与编辑接口不接受；
					`payload` 由平台按任务键写入 —— 二者都在「运行详情」里只读展示。
				</p>
			</form>
		</FormSheet>
	);
}
