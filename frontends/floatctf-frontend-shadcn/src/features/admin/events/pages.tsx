/**
 * `/admin/events` —— 赛事列表 / 新建 / 编辑 / 删除。
 *
 * 数据全部来自 `client.admin.events.*`；`filter` 用后端 `apply_filters` 的
 * `key:value` 语法（`title` / `family` / `is_virtual` 均为后端已声明的过滤键）。
 */

import { useMemo, useState, type ReactNode } from "react";
import { Link, useNavigate } from "react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { MoreHorizontal, Plus, RefreshCw, Trash2 } from "lucide-react";

import { call, callList } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { BooleanPill, TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import type { DataTableColumn } from "~/components/app/data-table";
import { PageBody, PageHeader, SectionCard, Toolbar } from "~/components/app/page";
import { EmptyBlock } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
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
import { EventFamily } from "@floatctf/sdk/entity";
import type { Events } from "@floatctf/sdk/entity";
import { formatRange } from "~/lib/format";
import { EVENT_FAMILY_LABEL } from "~/lib/event-status";
import { useDebouncedValue, useDocumentTitle } from "~/lib/hooks";

import { EventFormSheet } from "./event-form";
import {
	EventFamilyPill,
	EventPurposePills,
	EventStatusPill,
	MobileFacts,
	PagedTable,
	ParticipantModePill,
} from "./shared";

type Scope = "normal" | "virtual";

export function EventsListPage(): ReactNode {
	useDocumentTitle("赛事管理 · FloatCTF");
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const navigate = useNavigate();

	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(20);
	const [search, setSearch] = useState("");
	const [scope, setScope] = useState<Scope>("normal");
	const [family, setFamily] = useState<"all" | EventFamily>("all");
	const [selectedIds, setSelectedIds] = useState<string[]>([]);
	const [sheet, setSheet] = useState<{ event: Events | null } | null>(null);

	const keyword = useDebouncedValue(search, 300);

	const filter = useMemo(() => {
		const parts = [scope === "virtual" ? "is_virtual:true" : "is_virtual:false"];
		if (family !== "all") parts.push(`family:${family}`);
		const cleaned = keyword.replace(/[&|]/g, " ").trim();
		if (cleaned !== "") parts.push(`title:${cleaned}`);
		return parts.join(" & ");
	}, [scope, family, keyword]);

	const query = useQuery({
		queryKey: qk.admin.events({ page, limit: pageSize, filter }),
		queryFn: () => callList<Events>(client.admin.events.fetch({ page, limit: pageSize, filter })),
	});

	const removeEvents = useMutation({
		mutationFn: (ids: string[]) => call<number>(client.admin.events.remove(ids), "删除赛事"),
		onSuccess: (count) => {
			toast.success(`已删除 ${count} 个赛事`);
			setSelectedIds([]);
			void queryClient.invalidateQueries({ queryKey: qk.admin.events() });
			void queryClient.invalidateQueries({ queryKey: qk.admin.dashboard() });
		},
		onError: (error) => toast.apiError("删除赛事失败", error),
	});

	async function confirmRemove(targets: Events[]) {
		if (targets.length === 0) return;
		const single = targets.length === 1 ? targets[0] : null;
		const ok = await confirm({
			title: single ? `删除赛事「${single.title}」？` : `删除所选 ${targets.length} 个赛事？`,
			description: "赛事删除不可恢复，后端会同时拆除对应的运行时资源。",
			consequences: [
				"赛事行被删除：参赛选手 / 战队、赛事挂题、公告、日志、Write-up 与实例记录随外键级联删除",
				"解题记录（jeopardy_challenge_solves）随赛事级联删除，赛事积分与排行不再可查",
				"AWD / AWDP 运行时被拆除：容器、赛事专属 Docker 网络、WireGuard 接口与 nftables 规则一并回收",
				"系统托管赛事（system_key 非空）会被后端拒绝删除，不会被误删",
			],
			tone: "danger",
			confirmText: "删除赛事",
			confirmPhrase: single ? single.title : "delete",
		});
		if (!ok) return;
		removeEvents.mutate(targets.map((item) => item.id));
	}

	const columns: DataTableColumn<Events>[] = [
		{
			id: "title",
			header: "标题",
			cell: (row) => (
				<div className="min-w-0">
					<Link
						to={`/admin/events/${row.id}`}
						className="font-medium underline-offset-4 hover:underline"
					>
						{row.title}
					</Link>
					<div className="mt-0.5 flex flex-wrap items-center gap-1">
						{row.hidden ? <TonePill tone="muted">隐藏</TonePill> : null}
						{row.system_key ? <TonePill tone="warning">系统托管</TonePill> : null}
					</div>
				</div>
			),
			sortValue: (row) => row.title,
		},
		{
			id: "family",
			header: "家族",
			cell: (row) => <EventFamilyPill family={row.family} />,
			sortValue: (row) => EVENT_FAMILY_LABEL[row.family] ?? row.family,
		},
		{
			id: "participant_mode",
			header: "赛制",
			cell: (row) => <ParticipantModePill mode={row.participant_mode} />,
			sortValue: (row) => row.participant_mode,
		},
		{
			id: "window",
			header: "时间窗",
			cell: (row) => (
				<span className="tnum text-xs">{formatRange(row.start_time, row.end_time ?? null)}</span>
			),
			sortValue: (row) => Date.parse(row.start_time),
			hideBelow: "lg",
		},
		{
			id: "status",
			header: "状态",
			cell: (row) => <EventStatusPill event={row} />,
			sortValue: (row) => row.start_time,
		},
		{
			id: "hidden",
			header: "可见性",
			cell: (row) => (
				<BooleanPill
					value={row.hidden}
					trueText="已隐藏"
					falseText="公开"
					trueTone="muted"
					falseTone="success"
				/>
			),
			sortValue: (row) => (row.hidden ? 1 : 0),
			hideBelow: "md",
		},
		{
			id: "purpose",
			header: "用途 / 虚拟",
			cell: (row) => <EventPurposePills event={row} />,
			sortValue: (row) => `${row.purpose}-${row.is_virtual ? "v" : "n"}`,
			hideBelow: "lg",
		},
	];

	const rowActions = (row: Events): ReactNode => (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button variant="ghost" size="icon-sm" aria-label={`${row.title} 的操作`}>
					<MoreHorizontal />
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="end">
				<DropdownMenuLabel className="max-w-56 truncate">{row.title}</DropdownMenuLabel>
				<DropdownMenuItem onSelect={() => void navigate(`/admin/events/${row.id}`)}>
					打开控制台
				</DropdownMenuItem>
				<DropdownMenuItem onSelect={() => setSheet({ event: row })}>编辑赛事</DropdownMenuItem>
				<DropdownMenuSeparator />
				<DropdownMenuItem variant="destructive" onSelect={() => void confirmRemove([row])}>
					<Trash2 /> 删除赛事
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	);

	const mobileCard = (row: Events): ReactNode => (
		<div className="space-y-2">
			<MobileFacts
				items={[
					{
						k: "标题",
						v: (
							<Link
								to={`/admin/events/${row.id}`}
								className="font-medium underline-offset-4 hover:underline"
							>
								{row.title}
							</Link>
						),
					},
					{ k: "家族", v: <EventFamilyPill family={row.family} /> },
					{ k: "赛制", v: <ParticipantModePill mode={row.participant_mode} /> },
					{ k: "状态", v: <EventStatusPill event={row} /> },
					{
						k: "时间窗",
						v: (
							<span className="tnum text-xs">
								{formatRange(row.start_time, row.end_time ?? null)}
							</span>
						),
					},
					{ k: "用途", v: <EventPurposePills event={row} /> },
					{
						k: "可见性",
						v: (
							<BooleanPill
								value={row.hidden}
								trueText="已隐藏"
								falseText="公开"
								trueTone="muted"
								falseTone="success"
							/>
						),
					},
				]}
			/>
			<div className="flex justify-end">
				<Button variant="outline" size="xs" asChild>
					<Link to={`/admin/events/${row.id}`}>打开控制台</Link>
				</Button>
			</div>
		</div>
	);

	return (
		<PageBody>
			<PageHeader
				title="赛事管理"
				description="一个赛事 = 一个控制台：配置、题目、成员、战队、公告、实例、日志与运行时概览都在同一工作区。"
				actions={
					<Toolbar>
						<Button
							variant="outline"
							onClick={() => {
								void queryClient.invalidateQueries({ queryKey: qk.admin.events() });
							}}
							disabled={query.isFetching}
						>
							<RefreshCw className={query.isFetching ? "animate-spin" : undefined} /> 刷新
						</Button>
						<Button
							onClick={() => setSheet({ event: null })}
							disabled={scope === "virtual"}
							title={
								scope === "virtual"
									? "虚拟（练习）赛事由平台 / AWDP 流程创建，不能在管理端手动新建"
									: undefined
							}
						>
							<Plus /> 新建赛事
						</Button>
					</Toolbar>
				}
			/>

			<SectionCard
				title={scope === "virtual" ? "虚拟（练习）赛事" : "正式赛事"}
				description={
					scope === "virtual"
						? "purpose=practice / is_virtual=true 的赛事，由平台或 AWDP 流程创建。"
						: "purpose=competition / is_virtual=false 的赛事。"
				}
				actions={
					<Toolbar>
						<Input
							value={search}
							onChange={(changeEvent) => {
								setSearch(changeEvent.target.value);
								setPage(1);
							}}
							placeholder="搜索标题…"
							className="w-48"
							aria-label="搜索赛事标题"
						/>
						<Select
							value={family}
							onValueChange={(value) => {
								setFamily(value as "all" | EventFamily);
								setPage(1);
							}}
						>
							<SelectTrigger size="sm" className="w-40" aria-label="按家族筛选">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value="all">全部家族</SelectItem>
								<SelectItem value={EventFamily.Jeopardy}>
									{EVENT_FAMILY_LABEL[EventFamily.Jeopardy]}
								</SelectItem>
								<SelectItem value={EventFamily.Awd}>
									{EVENT_FAMILY_LABEL[EventFamily.Awd]}
								</SelectItem>
								<SelectItem value={EventFamily.Awdp}>
									{EVENT_FAMILY_LABEL[EventFamily.Awdp]}
								</SelectItem>
							</SelectContent>
						</Select>
						<Select
							value={scope}
							onValueChange={(value) => {
								setScope(value as Scope);
								setPage(1);
								setSelectedIds([]);
							}}
						>
							<SelectTrigger size="sm" className="w-36" aria-label="赛事类型">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value="normal">正式赛事</SelectItem>
								<SelectItem value="virtual">虚拟赛事</SelectItem>
							</SelectContent>
						</Select>
						{selectedIds.length > 0 ? (
							<Button
								variant="destructive"
								size="sm"
								onClick={() => {
									const targets = (query.data?.items ?? []).filter((item) =>
										selectedIds.includes(item.id),
									);
									void confirmRemove(targets);
								}}
							>
								<Trash2 /> 删除所选（{selectedIds.length}）
							</Button>
						) : null}
					</Toolbar>
				}
			>
				<PagedTable
					query={query}
					columns={columns}
					getRowId={(row) => row.id}
					page={page}
					pageSize={pageSize}
					onPageChange={setPage}
					onPageSizeChange={(size) => {
						setPageSize(size);
						setPage(1);
					}}
					rowActions={rowActions}
					selectable
					selectedIds={selectedIds}
					onSelectionChange={setSelectedIds}
					mobileCard={mobileCard}
					errorTitle="加载赛事列表失败"
					empty={
						<EmptyBlock
							title="没有匹配的赛事"
							description={
								scope === "virtual"
									? "当前没有虚拟（练习）赛事。"
									: "还没有正式赛事，点右上角「新建赛事」创建第一场。"
							}
							variant={search.trim() === "" && family === "all" ? "empty" : "filtered"}
						/>
					}
				/>
			</SectionCard>

			{sheet ? (
				<EventFormSheet
					open
					onOpenChange={(next) => {
						if (!next) setSheet(null);
					}}
					event={sheet.event}
				/>
			) : null}

			<p className="text-xs text-muted-foreground">
				列表按后端 updated_at 倒序返回；时间窗与状态由 start_time / end_time 推导。
			</p>
		</PageBody>
	);
}
