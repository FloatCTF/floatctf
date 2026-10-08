/**
 * 管理端 · 动态设置（`/admin/platform/settings`，required 能力）。
 *
 * 真实接口：`client.admin.settings.fetch()` / `create` / `patch(带 id)` / `remove(id_list)`。
 * 后端语义（`apps/api/src/modules/platform/settings/api.rs`）：
 * - `GET /admin/settings` **不接受任何 QueryParams**（SDK 签名也是 `fetch()`），
 *   一次性返回全部设置并已算好 `resolved_value` ⇒ 搜索与分页在客户端完成；
 * - `create` 必须带全 `key/value/description/protected/type`；
 * - `patch` 走 `PATCH /settings/{id}`（SDK 要求带 `.id`）；
 * - **受保护（protected）设置后端禁止删除**（返回 400），只能改值；
 * - `FRONTEND_ACTIVE` 的值必须是安全前端 ID（`[a-z0-9][a-z0-9._-]*`，≤64）。
 */

import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
	ExternalLink,
	Lock,
	MoreHorizontal,
	Pencil,
	Plus,
	RefreshCw,
	ShieldAlert,
	Trash2,
} from "lucide-react";
import { Link } from "react-router";

import type { SettingsDto } from "@floatctf/sdk";
import { SettingValueType } from "@floatctf/sdk/entity";

import { call, callList, callVoid } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { BooleanPill, TonePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { Field, FormFooter, FormGrid, FormSheet } from "~/components/app/form";
import {
	CopyText,
	MonoText,
	PageBody,
	PageHeader,
	PaginationBar,
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
import { Switch } from "~/components/ui/switch";
import { Textarea } from "~/components/ui/textarea";
import { truncate } from "~/lib/format";
import { useDocumentTitle } from "~/lib/hooks";

import {
	DeleteSelectedButton,
	OutOfRangeBlock,
	SearchInput,
	pageSlice,
	useListState,
} from "./components";

const PAGE_SIZE_OPTIONS = [10, 20, 50];

/** 平台关键设置：切换生效前端（只读提示 + 跳转选择器）。 */
const FRONTEND_ACTIVE_KEY = "FRONTEND_ACTIVE";

/** 覆盖受保护设置值所需的确认词。 */
const OVERRIDE_PHRASE = "OVERRIDE";

const TYPE_LABEL: Record<string, string> = {
	[SettingValueType.String]: "字符串",
	[SettingValueType.Integer]: "整数",
	[SettingValueType.Boolean]: "布尔",
	[SettingValueType.Float]: "浮点",
};

export function AdminSettingsPage(): ReactNode {
	useDocumentTitle("动态设置 · FloatCTF 控制台");
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const list = useListState(20);
	const [selected, setSelected] = useState<string[]>([]);
	const [editing, setEditing] = useState<SettingsDto | null | undefined>(undefined);

	const query = useQuery({
		queryKey: qk.admin.settings({ all: true }),
		queryFn: () => callList<SettingsDto>(client.admin.settings.fetch()),
	});

	const all = query.data?.items ?? [];

	const removeMutation = useMutation({
		mutationFn: (ids: string[]) =>
			callVoid(client.admin.settings.remove(ids), "删除设置"),
		onSuccess: (_data, ids) => {
			toast.success(`已删除 ${ids.length} 条设置`);
			setSelected([]);
			void queryClient.invalidateQueries({ queryKey: qk.admin.settings() });
		},
		onError: (error) => toast.apiError("删除设置失败", error),
	});

	async function removeRows(rows: SettingsDto[]) {
		const deletable = rows.filter((row) => !row.protected);
		const blocked = rows.filter((row) => row.protected);
		if (blocked.length > 0) {
			toast.warning(
				"受保护设置无法删除",
				`已跳过：${blocked.map((row) => row.key).join("、")}（后端会拒绝删除 protected 设置）`,
			);
		}
		if (deletable.length === 0) return;
		const ok = await confirm({
			title:
				deletable.length === 1
					? `删除设置 ${deletable[0].key}？`
					: `删除 ${deletable.length} 条设置？`,
			description: deletable.map((row) => row.key).join("、"),
			consequences: [
				"读取该键的平台功能会立即回落到内建默认值或报错",
				"若该键被其它设置引用（$KEY 形式），引用方解析结果会一并改变",
				"删除后不可恢复（需要重新创建并填写正确取值）",
			],
			tone: "danger",
			confirmText: "永久删除",
			confirmPhrase: "DELETE",
		});
		if (!ok) return;
		removeMutation.mutate(deletable.map((row) => row.id));
	}

	const keyword = list.filter.toLowerCase();
	const filtered = all.filter(
		(row) =>
			keyword.length === 0 ||
			row.key.toLowerCase().includes(keyword) ||
			row.value.toLowerCase().includes(keyword) ||
			(row.resolved_value ?? "").toLowerCase().includes(keyword) ||
			row.description.toLowerCase().includes(keyword),
	);
	const visible = pageSlice(filtered, list.page, list.pageSize);
	const protectedCount = all.filter((row) => row.protected).length;

	useEffect(() => {
		setSelected([]);
	}, [list.page, list.filter]);

	const columns: DataTableColumn<SettingsDto>[] = [
		{
			id: "key",
			header: "键",
			sortValue: (row) => row.key,
			cell: (row) => (
				<span className="inline-flex items-center gap-1.5">
					<MonoText className="font-medium">{row.key}</MonoText>
					{row.key === FRONTEND_ACTIVE_KEY ? (
						<TonePill tone="info">生效前端</TonePill>
					) : null}
					{row.protected ? <Lock className="size-3 text-muted-foreground" /> : null}
				</span>
			),
		},
		{
			id: "value",
			header: "值",
			cell: (row) => <MonoText>{truncate(row.value, 40) || "—"}</MonoText>,
		},
		{
			id: "resolved_value",
			header: "解析值",
			hideBelow: "lg",
			cell: (row) => (
				<MonoText className="text-muted-foreground">
					{truncate(row.resolved_value ?? "", 40) || "—"}
				</MonoText>
			),
		},
		{
			id: "type",
			header: "类型",
			hideBelow: "sm",
			sortValue: (row) => row.type,
			cell: (row) => <TonePill tone="muted">{TYPE_LABEL[row.type] ?? row.type}</TonePill>,
		},
		{
			id: "protected",
			header: "受保护",
			hideBelow: "md",
			cell: (row) => <BooleanPill value={row.protected} trueText="受保护" falseText="可删除" />,
		},
		{
			id: "description",
			header: "描述",
			hideBelow: "xl",
			cell: (row) => (
				<span className="text-muted-foreground">
					{truncate(row.description ?? "", 50) || "—"}
				</span>
			),
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
				title="动态设置"
				description="平台运行时配置（动态设置）。受保护键只能改值，后端禁止删除。"
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
							新建设置
						</Button>
					</Toolbar>
				}
			/>

			<SectionCard
				title="设置列表"
				description={`共 ${all.length} 条（其中受保护 ${protectedCount} 条）。设置接口不接受分页/过滤参数，搜索与分页在本页完成。`}
				actions={
					<div className="flex flex-wrap items-center gap-2">
						<SearchInput
							value={list.search}
							onChange={list.setSearch}
							placeholder="搜索键 / 值 / 描述"
						/>
						<DeleteSelectedButton
							count={selected.length}
							isPending={removeMutation.isPending}
							label="删除所选设置"
							onClick={() => void removeRows(all.filter((row) => selected.includes(row.id)))}
						/>
					</div>
				}
			>
				<QueryState
					query={query}
					skeleton={<TableSkeleton rows={8} columns={6} />}
					errorTitle="加载设置失败"
					isEmpty={(result) => result.items.length === 0}
					empty={
						<EmptyBlock
							title="暂无设置"
							description="平台启动时会补种默认设置；若此处为空请检查后端初始化。"
						/>
					}
				>
					{() =>
						filtered.length === 0 ? (
							<EmptyBlock
								variant="filtered"
								title="没有匹配的设置"
								description="试试调整搜索关键字。"
							/>
						) : visible.length === 0 ? (
							<OutOfRangeBlock onReset={() => list.setPage(1)} />
						) : (
							<div className="space-y-3">
								<DataTable
									data={visible}
									getRowId={(row) => row.id}
									columns={columns}
									selectable
									selectedIds={selected}
									onSelectionChange={setSelected}
									rowActions={(row) => (
										<DropdownMenu>
											<DropdownMenuTrigger asChild>
												<Button variant="ghost" size="icon-sm" aria-label="设置操作">
													<MoreHorizontal />
												</Button>
											</DropdownMenuTrigger>
											<DropdownMenuContent align="end">
												<DropdownMenuItem onSelect={() => setEditing(row)}>
													<Pencil />
													{row.protected ? "改值（需二次确认）" : "编辑"}
												</DropdownMenuItem>
												{row.key === FRONTEND_ACTIVE_KEY ? (
													<DropdownMenuItem asChild>
														<Link to="/admin/platform/frontends">
															<ExternalLink />
															去前端选择器
														</Link>
													</DropdownMenuItem>
												) : null}
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
												<MonoText className="font-medium">{row.key}</MonoText>
												<AbsoluteTime value={row.updated_at} />
											</div>
											<p className="flex flex-wrap items-center gap-1.5 text-sm">
												<MonoText>{truncate(row.value, 40) || "—"}</MonoText>
												<BooleanPill
													value={row.protected}
													trueText="受保护"
													falseText="可删除"
												/>
											</p>
											<p className="text-xs text-muted-foreground">
												{truncate(row.description ?? "", 80) || "（无描述）"}
											</p>
											<div className="flex items-center gap-2">
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
									total={filtered.length}
									onPageChange={list.setPage}
									onPageSizeChange={list.setPageSize}
									pageSizeOptions={PAGE_SIZE_OPTIONS}
								/>
							</div>
						)
					}
				</QueryState>
			</SectionCard>

			<SettingFormSheet
				open={editing !== undefined}
				row={editing ?? null}
				onOpenChange={(open) => {
					if (!open) setEditing(undefined);
				}}
			/>
		</PageBody>
	);
}

function SettingFormSheet({
	open,
	row,
	onOpenChange,
}: {
	open: boolean;
	row: SettingsDto | null;
	onOpenChange: (open: boolean) => void;
}): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const [form, setForm] = useState<{
		key: string;
		value: string;
		description: string;
		type: SettingValueType;
		protected: boolean;
	}>({
		key: "",
		value: "",
		description: "",
		type: SettingValueType.String,
		protected: false,
	});
	const [errors, setErrors] = useState<{ key?: string; value?: string }>({});

	useEffect(() => {
		if (!open) return;
		setErrors({});
		setForm({
			key: row?.key ?? "",
			value: row?.value ?? "",
			description: row?.description ?? "",
			type: row?.type ?? SettingValueType.String,
			protected: row?.protected ?? false,
		});
	}, [open, row]);

	function invalidate() {
		void queryClient.invalidateQueries({ queryKey: qk.admin.settings() });
	}

	const createMutation = useMutation({
		mutationFn: (input: Partial<SettingsDto>) =>
			call(client.admin.settings.create(input), "创建设置"),
		onSuccess: () => {
			toast.success("设置已创建");
			invalidate();
			onOpenChange(false);
		},
		onError: (error) => toast.apiError("创建设置失败", error),
	});

	const patchMutation = useMutation({
		mutationFn: (input: Partial<SettingsDto>) =>
			call(client.admin.settings.patch(input), "更新设置"),
		onSuccess: () => {
			toast.success("设置已更新");
			invalidate();
			onOpenChange(false);
		},
		onError: (error) => toast.apiError("更新设置失败", error),
	});

	const isPending = createMutation.isPending || patchMutation.isPending;

	async function submit(event: FormEvent<HTMLFormElement>) {
		event.preventDefault();
		const nextKey = form.key.trim();
		const nextErrors: { key?: string; value?: string } = {};
		if (nextKey.length === 0) nextErrors.key = "键不能为空";
		if (form.value.length === 0) nextErrors.value = "值不能为空";
		setErrors(nextErrors);
		if (nextErrors.key || nextErrors.value) return;

		// 受保护设置的「覆盖」需要额外确认（真实后果：影响所有读取该键的功能）。
		if (row?.protected) {
			const changedValue = row.value !== form.value;
			const changedKey = row.key !== nextKey;
			const ok = await confirm({
				title: `覆盖受保护设置 ${row.key}？`,
				description: changedKey
					? `键名将从 ${row.key} 改为 ${nextKey}`
					: changedValue
						? `值将从「${truncate(row.value, 40)}」改为「${truncate(form.value, 40)}」`
						: "仅描述 / 类型等元数据变化",
				consequences: [
					"受保护设置由平台自身依赖，改错会导致对应功能行为异常甚至不可用",
					row.key === FRONTEND_ACTIVE_KEY
						? "FRONTEND_ACTIVE 决定所有用户下次加载的界面，且值必须是已安装的安全前端 ID"
						: "修改立即失效缓存并对所有请求生效，不会自动回滚",
					"后端不会额外拦截受保护键的改值，请确认取值正确",
				],
				tone: "danger",
				confirmText: "覆盖",
				confirmPhrase: OVERRIDE_PHRASE,
			});
			if (!ok) return;
		}

		const payload: Partial<SettingsDto> = {
			key: nextKey,
			value: form.value,
			description: form.description,
			type: form.type,
			protected: form.protected,
		};
		if (row) patchMutation.mutate({ ...payload, id: row.id });
		else createMutation.mutate(payload);
	}

	return (
		<FormSheet
			open={open}
			onOpenChange={onOpenChange}
			title={row ? `编辑设置 ${row.key}` : "新建设置"}
			description={
				row?.protected
					? "这是受保护设置：保存时需要输入确认词，且后端不允许删除。"
					: "键名唯一；值为字符串，平台按类型解析。"
			}
			width="lg"
			footer={
				<FormFooter
					formId="setting-form"
					isPending={isPending}
					submitLabel={row ? "保存" : "创建"}
					onCancel={() => onOpenChange(false)}
					hint={row ? `ID ${row.id}` : "键与值为必填项"}
				/>
			}
		>
			<form id="setting-form" className="space-y-4" onSubmit={(event) => void submit(event)}>
				{row?.protected ? (
					<div className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs">
						<ShieldAlert className="mt-0.5 size-4 shrink-0 text-destructive" />
						<span>
							受保护键（protected=true）：可以改值但**不能删除**；保存前需二次确认并输入{" "}
							<span className="font-mono">{OVERRIDE_PHRASE}</span>。
						</span>
					</div>
				) : null}
				<FormGrid columns={2}>
					<Field label="键" htmlFor="setting-key" required error={errors.key}>
						<Input
							id="setting-key"
							className="font-mono text-xs"
							autoComplete="off"
							value={form.key}
							onChange={(event) => setForm({ ...form, key: event.target.value })}
							placeholder="例如 FRONTEND_ACTIVE"
						/>
					</Field>
					<Field label="类型" htmlFor="setting-type" hint="平台按该类型解析值。">
						<Select
							value={form.type}
							onValueChange={(value) =>
								setForm({ ...form, type: value as SettingValueType })
							}
						>
							<SelectTrigger id="setting-type" className="w-full">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{Object.values(SettingValueType).map((type) => (
									<SelectItem key={type} value={type}>
										{TYPE_LABEL[type] ?? type}（{type}）
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</Field>
				</FormGrid>
				<Field
					label="值"
					htmlFor="setting-value"
					required
					error={errors.value}
					hint={
						form.key === FRONTEND_ACTIVE_KEY
							? "FRONTEND_ACTIVE 必须是安全前端 ID：[a-z0-9][a-z0-9._-]*，最长 64；建议用前端选择器。"
							: "支持 $OTHER_KEY 形式的引用（后端在读取时解析为 resolved_value）。"
					}
				>
					<Textarea
						id="setting-value"
						className="font-mono text-sm"
						style={{ minHeight: 90 }}
						value={form.value}
						onChange={(event) => setForm({ ...form, value: event.target.value })}
					/>
				</Field>
				<Field label="描述" htmlFor="setting-description">
					<Textarea
						id="setting-description"
						style={{ minHeight: 80 }}
						value={form.description}
						onChange={(event) => setForm({ ...form, description: event.target.value })}
					/>
				</Field>
				<Field
					label="受保护"
					htmlFor="setting-protected"
					hint="受保护键不可删除；关闭保护后即可删除，请谨慎操作。"
				>
					<div className="flex items-center gap-2">
						<Switch
							id="setting-protected"
							checked={form.protected}
							onCheckedChange={(checked) => setForm({ ...form, protected: checked })}
						/>
						<BooleanPill value={form.protected} trueText="受保护" falseText="可删除" />
					</div>
				</Field>
				{row ? (
					<div className="flex items-center gap-2 text-xs text-muted-foreground">
						<span>当前解析值：</span>
						<CopyText value={row.resolved_value ?? ""} label="解析值" />
					</div>
				) : null}
			</form>
		</FormSheet>
	);
}
