/**
 * 管理端 · 武器库治理（`/admin/community/weapons`）。
 *
 * 真实接口：`client.admin.weapons.fetch/create/patch/remove` + 附件上传。
 *
 * 两个必须遵守的事实：
 * 1. `GET /api/admin/weapons` **忽略分页与过滤参数**、直接返回全部武器且不带 `meta`
 *    （`apps/api/src/modules/weapon/api.rs::get_weapons_admin`），
 *    因此搜索与分页在本页做客户端处理。
 * 2. 上传为 `POST /weapons/{id}/upload`，multipart 字段名 **`weapon`**；SDK 门面
 *    `weapons.upload(weapon_id, file)` 不暴露进度，因此走公共上传逃生舱
 *    `uploadFile({ scope: "admin", ... })`（axios 级 `onUploadProgress`）。
 */

import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
	Download,
	ExternalLink,
	MoreHorizontal,
	Pencil,
	Plus,
	RefreshCw,
	Trash2,
	Upload,
} from "lucide-react";

import type { Weapons } from "@floatctf/sdk/entity/weapons";

import { call, callList, callVoid, uploadFile } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { BooleanPill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { Field, FormFooter, FormGrid, FormSheet } from "~/components/app/form";
import {
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
import { Switch } from "~/components/ui/switch";
import { Textarea } from "~/components/ui/textarea";
import { formatInt, truncate } from "~/lib/format";
import { useDocumentTitle } from "~/lib/hooks";

import { DeleteSelectedButton, SearchInput, pageSlice, useListState } from "./components";

const PAGE_SIZE_OPTIONS = [10, 20, 50];

/** 武器附件的同源公开地址（后端把 S3 key 存在 `file_url`，静态前缀为 `/public/`）。 */
function weaponFileHref(fileUrl: string): string {
	return `/public/${fileUrl.replace(/^\/+/, "")}`;
}

export function AdminWeaponsPage(): ReactNode {
	useDocumentTitle("武器库治理 · FloatCTF 控制台");
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const list = useListState(20);
	const [selected, setSelected] = useState<string[]>([]);
	const [editing, setEditing] = useState<Weapons | null | undefined>(undefined);

	const query = useQuery({
		queryKey: qk.admin.weapons({ all: true }),
		// 后端忽略 QueryParams 并返回全部：这里显式不带参数，避免"以为在服务端分页"。
		queryFn: () => callList<Weapons>(client.admin.weapons.fetch()),
	});

	const removeMutation = useMutation({
		mutationFn: (ids: string[]) => callVoid(client.admin.weapons.remove(ids), "删除武器"),
		onSuccess: (_data, ids) => {
			toast.success(`已删除 ${ids.length} 条武器`);
			setSelected([]);
			void queryClient.invalidateQueries({ queryKey: qk.admin.weapons() });
			void queryClient.invalidateQueries({ queryKey: qk.weapons.all });
		},
		onError: (error) => toast.apiError("删除武器失败", error),
	});

	async function removeRows(rows: Weapons[]) {
		if (rows.length === 0) return;
		const ok = await confirm({
			title: rows.length === 1 ? "删除这件武器？" : `删除 ${rows.length} 件武器？`,
			description:
				rows.length === 1
					? rows[0].name
					: `包含：${rows.slice(0, 3).map((row) => row.name).join("、")}${
							rows.length > 3 ? ` 等 ${rows.length} 件` : ""
						}`,
			consequences: [
				"选手端武器库会立即移除这些条目，已下载的文件仍留在对象存储中但不再有入口",
				"删除后不可恢复",
			],
			tone: "danger",
			confirmText: "删除",
		});
		if (!ok) return;
		removeMutation.mutate(rows.map((row) => row.id));
	}

	const all = query.data?.items ?? [];
	const keyword = list.filter.toLowerCase();
	const filtered = all.filter((row) => {
		if (keyword.length === 0) return true;
		return (
			row.name.toLowerCase().includes(keyword) ||
			row.category.toLowerCase().includes(keyword) ||
			(row.description ?? "").toLowerCase().includes(keyword)
		);
	});
	const visible = pageSlice(filtered, list.page, list.pageSize);

	useEffect(() => {
		setSelected([]);
	}, [list.page, list.filter]);

	const columns: DataTableColumn<Weapons>[] = [
		{
			id: "name",
			header: "名称",
			sortValue: (row) => row.name,
			cell: (row) => <span className="font-medium">{row.name}</span>,
		},
		{
			id: "category",
			header: "分类",
			sortValue: (row) => row.category,
			cell: (row) => <span>{row.category}</span>,
		},
		{
			id: "description",
			header: "描述",
			hideBelow: "lg",
			cell: (row) => (
				<span className="text-muted-foreground">{truncate(row.description ?? "", 50) || "—"}</span>
			),
		},
		{
			id: "file",
			header: "附件",
			cell: (row) =>
				row.has_file && row.file_url ? (
					<a
						href={weaponFileHref(row.file_url)}
						target="_blank"
						rel="noopener noreferrer"
						className="inline-flex items-center gap-1 text-primary underline underline-offset-4"
					>
						<ExternalLink className="size-3" />
						{truncate(row.file_url, 28)}
					</a>
				) : (
					<BooleanPill value={row.has_file} trueText="已标记" falseText="无附件" />
				),
		},
		{
			id: "download_count",
			header: "下载",
			align: "right",
			hideBelow: "sm",
			sortValue: (row) => row.download_count,
			cell: (row) => <MonoText>{formatInt(row.download_count)}</MonoText>,
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
				title="武器库"
				description="平台共享的工具、脚本与利用代码。附件上传走对象存储，字段名为 weapon。"
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
							新建武器
						</Button>
					</Toolbar>
				}
			/>

			<SectionCard
				title="武器列表"
				description="搜索与分页在本页完成：管理端武器接口一次性返回全部条目且不返回分页 meta。"
				actions={
					<div className="flex flex-wrap items-center gap-2">
						<SearchInput
							value={list.search}
							onChange={list.setSearch}
							placeholder="搜索名称 / 分类 / 描述"
						/>
						<DeleteSelectedButton
							count={selected.length}
							isPending={removeMutation.isPending}
							onClick={() => void removeRows(all.filter((row) => selected.includes(row.id)))}
						/>
					</div>
				}
			>
				<QueryState
					query={query}
					skeleton={<TableSkeleton rows={6} columns={5} />}
					errorTitle="加载武器库失败"
					isEmpty={(result) => result.items.length === 0}
					empty={
						<EmptyBlock
							title="暂无武器"
							description="新建一件武器后，这里可以上传附件并管理下载入口。"
						/>
					}
				>
					{() =>
						filtered.length === 0 ? (
							<EmptyBlock
								variant="filtered"
								title="没有匹配的武器"
								description="试试调整搜索关键字。"
							/>
						) : visible.length === 0 ? (
							<EmptyBlock
								title="当前页没有数据"
								description="条目可能刚被删除，翻页状态需要重置。"
								action={
									<Button size="sm" variant="outline" onClick={() => list.setPage(1)}>
										回到第 1 页
									</Button>
								}
							/>
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
										<div className="flex items-center gap-1">
											<WeaponUploadAction weapon={row} />
											<DropdownMenu>
												<DropdownMenuTrigger asChild>
													<Button variant="ghost" size="icon-sm" aria-label="武器操作">
														<MoreHorizontal />
													</Button>
												</DropdownMenuTrigger>
												<DropdownMenuContent align="end">
													<DropdownMenuItem onSelect={() => setEditing(row)}>
														<Pencil />
														编辑
													</DropdownMenuItem>
													{row.has_file && row.file_url ? (
														<DropdownMenuItem asChild>
															<a
																href={weaponFileHref(row.file_url)}
																target="_blank"
																rel="noopener noreferrer"
															>
																<Download />
																打开附件
															</a>
														</DropdownMenuItem>
													) : null}
													<DropdownMenuItem
														variant="destructive"
														onSelect={() => void removeRows([row])}
													>
														<Trash2 />
														删除
													</DropdownMenuItem>
												</DropdownMenuContent>
											</DropdownMenu>
										</div>
									)}
									mobileCard={(row) => (
										<div className="space-y-2">
											<div className="flex items-start justify-between gap-2">
												<span className="font-medium">{row.name}</span>
												<AbsoluteTime value={row.updated_at} />
											</div>
											<p className="text-xs text-muted-foreground">
												{row.category} · 下载 {formatInt(row.download_count)}
											</p>
											<p className="text-sm text-muted-foreground">
												{truncate(row.description ?? "", 100) || "（无描述）"}
											</p>
											<div className="flex flex-wrap items-center gap-2">
												<WeaponUploadAction weapon={row} />
												<Button variant="ghost" size="sm" onClick={() => setEditing(row)}>
													<Pencil />
													编辑
												</Button>
												<Button
													variant="ghost"
													size="sm"
													className="text-destructive"
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

			<WeaponFormSheet
				open={editing !== undefined}
				row={editing ?? null}
				onOpenChange={(open) => {
					if (!open) setEditing(undefined);
				}}
			/>
		</PageBody>
	);
}

/** 行内上传附件：字段名 `weapon`，显示上传百分比。 */
function WeaponUploadAction({ weapon }: { weapon: Weapons }): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const inputRef = useRef<HTMLInputElement>(null);
	const [percent, setPercent] = useState<number | null>(null);

	async function handleFile(file: File) {
		setPercent(0);
		try {
			await uploadFile({
				client,
				scope: "admin",
				url: `/weapons/${weapon.id}/upload`,
				field: "weapon",
				file,
				onProgress: setPercent,
			});
			toast.success("附件已上传", `${weapon.name} · ${file.name}`);
			void queryClient.invalidateQueries({ queryKey: qk.admin.weapons() });
			void queryClient.invalidateQueries({ queryKey: qk.weapons.all });
		} catch (error) {
			toast.apiError("附件上传失败", error);
		} finally {
			setPercent(null);
		}
	}

	const uploading = percent !== null;

	return (
		<>
			<input
				ref={inputRef}
				type="file"
				className="hidden"
				disabled={uploading}
				onChange={(event) => {
					const file = event.target.files?.[0];
					event.target.value = "";
					if (file) void handleFile(file);
				}}
			/>
			<Button
				variant="ghost"
				size="sm"
				disabled={uploading}
				onClick={() => inputRef.current?.click()}
				title={weapon.has_file ? "重新上传附件（覆盖）" : "上传附件"}
			>
				<Upload />
				{uploading ? `${percent}%` : "上传"}
			</Button>
		</>
	);
}

function WeaponFormSheet({
	open,
	row,
	onOpenChange,
}: {
	open: boolean;
	row: Weapons | null;
	onOpenChange: (open: boolean) => void;
}): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const [form, setForm] = useState({
		name: "",
		category: "",
		description: "",
		hasFile: false,
		fileUrl: "",
		downloadCount: 0,
	});
	const [errors, setErrors] = useState<{ name?: string; category?: string }>({});

	useEffect(() => {
		if (!open) return;
		setErrors({});
		setForm({
			name: row?.name ?? "",
			category: row?.category ?? "",
			description: row?.description ?? "",
			hasFile: row?.has_file ?? false,
			fileUrl: row?.file_url ?? "",
			downloadCount: row?.download_count ?? 0,
		});
	}, [open, row]);

	function invalidate() {
		void queryClient.invalidateQueries({ queryKey: qk.admin.weapons() });
		void queryClient.invalidateQueries({ queryKey: qk.weapons.all });
	}

	const createMutation = useMutation({
		mutationFn: (input: Partial<Weapons>) =>
			call(client.admin.weapons.create(input), "创建武器"),
		onSuccess: () => {
			toast.success("武器已创建");
			invalidate();
			onOpenChange(false);
		},
		onError: (error) => toast.apiError("创建武器失败", error),
	});

	const patchMutation = useMutation({
		mutationFn: (input: Partial<Weapons>) =>
			call(client.admin.weapons.patch(input), "更新武器"),
		onSuccess: () => {
			toast.success("武器已更新");
			invalidate();
			onOpenChange(false);
		},
		onError: (error) => toast.apiError("更新武器失败", error),
	});

	const isPending = createMutation.isPending || patchMutation.isPending;

	function submit(event: FormEvent<HTMLFormElement>) {
		event.preventDefault();
		const name = form.name.trim();
		const category = form.category.trim();
		const nextErrors: { name?: string; category?: string } = {};
		if (name.length === 0) nextErrors.name = "名称不能为空";
		if (category.length === 0) nextErrors.category = "分类不能为空";
		setErrors(nextErrors);
		if (nextErrors.name || nextErrors.category) return;

		const payload: Partial<Weapons> = {
			name,
			category,
			description: form.description,
			has_file: form.hasFile,
			file_url: form.fileUrl.trim(),
			download_count: form.downloadCount,
		};
		if (row) patchMutation.mutate({ ...payload, id: row.id });
		else createMutation.mutate(payload);
	}

	return (
		<FormSheet
			open={open}
			onOpenChange={onOpenChange}
			title={row ? "编辑武器" : "新建武器"}
			description="附件请用列表行内的「上传」按钮单独上传（字段名 weapon）。"
			width="lg"
			footer={
				<FormFooter
					formId="weapon-form"
					isPending={isPending}
					submitLabel={row ? "保存" : "创建"}
					onCancel={() => onOpenChange(false)}
					hint={row ? `ID ${row.id}` : "名称与分类为必填项"}
				/>
			}
		>
			<form id="weapon-form" className="space-y-4" onSubmit={submit}>
				<FormGrid columns={2}>
					<Field label="名称" htmlFor="weapon-name" required error={errors.name}>
						<Input
							id="weapon-name"
							value={form.name}
							onChange={(event) => setForm({ ...form, name: event.target.value })}
						/>
					</Field>
					<Field label="分类" htmlFor="weapon-category" required error={errors.category}>
						<Input
							id="weapon-category"
							value={form.category}
							onChange={(event) => setForm({ ...form, category: event.target.value })}
							placeholder="例如 web / pwn / misc"
						/>
					</Field>
				</FormGrid>
				<Field label="描述" htmlFor="weapon-description">
					<Textarea
						id="weapon-description"
						value={form.description}
						onChange={(event) => setForm({ ...form, description: event.target.value })}
						style={{ minHeight: 120 }}
					/>
				</Field>
				<FormGrid columns={2}>
					<Field
						label="文件地址"
						htmlFor="weapon-file-url"
						hint="对象存储 key；上传附件后由后端写入（形如 weapons/xxx.zip）。"
					>
						<Input
							id="weapon-file-url"
							className="font-mono text-xs"
							value={form.fileUrl}
							onChange={(event) => setForm({ ...form, fileUrl: event.target.value })}
						/>
					</Field>
					<Field label="下载次数" htmlFor="weapon-download-count" hint="通常由选手端下载累加。">
						<Input
							id="weapon-download-count"
							type="number"
							min={0}
							value={String(form.downloadCount)}
							onChange={(event) =>
								setForm({ ...form, downloadCount: Number(event.target.value) || 0 })
							}
						/>
					</Field>
				</FormGrid>
				<Field label="含附件" htmlFor="weapon-has-file" hint="开启后选手端显示下载入口。">
					<div className="flex items-center gap-2">
						<Switch
							id="weapon-has-file"
							checked={form.hasFile}
							onCheckedChange={(checked) => setForm({ ...form, hasFile: checked })}
						/>
						<BooleanPill value={form.hasFile} trueText="有附件" falseText="无附件" />
					</div>
				</Field>
			</form>
		</FormSheet>
	);
}
