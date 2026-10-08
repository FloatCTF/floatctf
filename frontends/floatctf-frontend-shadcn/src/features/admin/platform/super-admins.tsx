/**
 * 管理端 · 超管账号治理（`/admin/platform/super-admins`）。
 *
 * 真实接口：`client.admin.super_admin.fetch/create/patch/remove`。
 * SDK/后端事实（`packages/sdk/src/api/admin/super_admin.ts` +
 * `apps/api/src/modules/identity/administrator/mod.rs`）：
 * - `patch(id, data)` 实际发 **POST** `/super_admin/{id}`；
 * - `remove(id_list)` 是 `DELETE` + body；
 * - 列表接口**没有 filter 映射**（只有 `order_by_desc(updated_at)`），搜索只能客户端做；
 * - 响应 DTO 含 `password`（Argon2 哈希）—— 本页**绝不渲染**该字段。
 */

import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { KeyRound, MoreHorizontal, Pencil, Plus, RefreshCw, ShieldOff, Trash2 } from "lucide-react";

import type { SuperAdmin } from "@floatctf/sdk/entity";

import { call, callList, callVoid } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
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
import { AbsoluteTime, UserAvatar } from "~/components/app/user-cell";
import { Button } from "~/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "~/components/ui/dropdown-menu";
import { Input } from "~/components/ui/input";
import { useDocumentTitle } from "~/lib/hooks";

import { DeleteSelectedButton, OutOfRangeBlock, SearchInput, pageSlice, useListState } from "./components";

const PAGE_SIZE_OPTIONS = [10, 20, 50];
const DELETE_PHRASE = "DELETE";

export function AdminSuperAdminsPage(): ReactNode {
	useDocumentTitle("超管账号 · FloatCTF 控制台");
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const list = useListState(20);
	const [selected, setSelected] = useState<string[]>([]);
	const [editing, setEditing] = useState<SuperAdmin | null | undefined>(undefined);

	const query = useQuery({
		queryKey: qk.admin.superAdmins({ all: true }),
		// 后端不支持 filter：一次取回全部（超管数量级极小），本页做搜索与分页。
		queryFn: () => callList<SuperAdmin>(client.admin.super_admin.fetch()),
	});

	const removeMutation = useMutation({
		mutationFn: (ids: string[]) =>
			callVoid(client.admin.super_admin.remove(ids), "删除管理员"),
		onSuccess: (_data, ids) => {
			toast.success(`已删除 ${ids.length} 个管理员账号`);
			setSelected([]);
			void queryClient.invalidateQueries({ queryKey: qk.admin.superAdmins() });
		},
		onError: (error) => toast.apiError("删除管理员失败", error),
	});

	async function removeRows(rows: SuperAdmin[]) {
		if (rows.length === 0) return;
		const names = rows.map((row) => row.username).join("、");
		const ok = await confirm({
			title: rows.length === 1 ? `删除管理员 ${rows[0].username}？` : `删除 ${rows.length} 个管理员？`,
			description: names,
			consequences: [
				"这些账号会**立即**失去管理端登录权限（已签发的 JWT 到期前仍有效）",
				"若删除的是唯一超管，控制台将无人可登录，只能通过数据库恢复",
				"删除后不可恢复；账号创建的历史操作日志仍会保留",
			],
			tone: "danger",
			confirmText: "永久删除",
			confirmPhrase: DELETE_PHRASE,
		});
		if (!ok) return;
		removeMutation.mutate(rows.map((row) => row.id));
	}

	const all = query.data?.items ?? [];
	const keyword = list.filter.toLowerCase();
	const filtered = all.filter(
		(row) =>
			keyword.length === 0 ||
			row.username.toLowerCase().includes(keyword) ||
			row.email.toLowerCase().includes(keyword),
	);
	const visible = pageSlice(filtered, list.page, list.pageSize);

	useEffect(() => {
		setSelected([]);
	}, [list.page, list.filter]);

	const columns: DataTableColumn<SuperAdmin>[] = [
		{
			id: "username",
			header: "管理员",
			sortValue: (row) => row.username,
			cell: (row) => (
				<div className="flex items-center gap-2">
					<UserAvatar name={row.username} size="sm" />
					<span className="font-medium">{row.username}</span>
				</div>
			),
		},
		{
			id: "email",
			header: "邮箱",
			sortValue: (row) => row.email,
			cell: (row) => <span>{row.email || "—"}</span>,
		},
		{
			id: "password",
			header: "凭据",
			hideBelow: "md",
			cell: () => (
				<MonoText className="text-muted-foreground" title="后端只返回 Argon2 哈希，界面不展示">
					••••••••
				</MonoText>
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
				title="超管账号"
				description="管理端登录账号。密码只以 Argon2 哈希存储，界面永不展示。"
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
							新建管理员
						</Button>
					</Toolbar>
				}
			/>

			<SectionCard
				title="账号列表"
				description="后端列表接口不支持服务端过滤，本页在已加载的账号上做搜索与分页。"
				actions={
					<div className="flex flex-wrap items-center gap-2">
						<SearchInput
							value={list.search}
							onChange={list.setSearch}
							placeholder="搜索用户名 / 邮箱"
						/>
						<DeleteSelectedButton
							count={selected.length}
							isPending={removeMutation.isPending}
							label="删除所选账号"
							onClick={() => void removeRows(all.filter((row) => selected.includes(row.id)))}
						/>
					</div>
				}
			>
				<QueryState
					query={query}
					skeleton={<TableSkeleton rows={4} columns={4} />}
					errorTitle="加载管理员列表失败"
					isEmpty={(result) => result.items.length === 0}
					empty={
						<EmptyBlock
							title="没有管理员账号"
							description="平台启动时会自动补种默认超管；若这里为空请检查后端初始化。"
						/>
					}
				>
					{() =>
						filtered.length === 0 ? (
							<EmptyBlock
								variant="filtered"
								title="没有匹配的账号"
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
												<Button variant="ghost" size="icon-sm" aria-label="管理员操作">
													<MoreHorizontal />
												</Button>
											</DropdownMenuTrigger>
											<DropdownMenuContent align="end">
												<DropdownMenuItem onSelect={() => setEditing(row)}>
													<Pencil />
													编辑 / 改密
												</DropdownMenuItem>
												<DropdownMenuItem
													variant="destructive"
													onSelect={() => void removeRows([row])}
												>
													<Trash2 />
													删除
												</DropdownMenuItem>
											</DropdownMenuContent>
										</DropdownMenu>
									)}
									mobileCard={(row) => (
										<div className="space-y-2">
											<div className="flex items-start justify-between gap-2">
												<span className="font-medium">{row.username}</span>
												<AbsoluteTime value={row.updated_at} />
											</div>
											<p className="text-sm text-muted-foreground">{row.email || "—"}</p>
											<div className="flex items-center gap-2">
												<Button variant="ghost" size="sm" onClick={() => setEditing(row)}>
													<KeyRound />
													编辑 / 改密
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

			<SuperAdminFormSheet
				open={editing !== undefined}
				row={editing ?? null}
				onOpenChange={(open) => {
					if (!open) setEditing(undefined);
				}}
			/>
		</PageBody>
	);
}

function SuperAdminFormSheet({
	open,
	row,
	onOpenChange,
}: {
	open: boolean;
	row: SuperAdmin | null;
	onOpenChange: (open: boolean) => void;
}): ReactNode {
	const client = useClient();
	const queryClient = useQueryClient();
	const [username, setUsername] = useState("");
	const [email, setEmail] = useState("");
	const [password, setPassword] = useState("");
	const [errors, setErrors] = useState<{ username?: string; password?: string }>({});

	useEffect(() => {
		if (!open) return;
		setUsername(row?.username ?? "");
		setEmail(row?.email ?? "");
		setPassword("");
		setErrors({});
	}, [open, row]);

	const createMutation = useMutation({
		mutationFn: (input: { username: string; password: string; email: string }) =>
			call(client.admin.super_admin.create(input), "创建管理员"),
		onSuccess: () => {
			toast.success("管理员已创建");
			void queryClient.invalidateQueries({ queryKey: qk.admin.superAdmins() });
			onOpenChange(false);
		},
		onError: (error) => toast.apiError("创建管理员失败", error),
	});

	const patchMutation = useMutation({
		// 注意：SDK 的 `patch(id, data)` 实际发 POST /super_admin/{id}；不带 password 即不改密码。
		mutationFn: (input: { id: string; username: string; email: string; password?: string }) => {
			const { id, ...data } = input;
			return call(client.admin.super_admin.patch(id, data), "更新管理员");
		},
		onSuccess: () => {
			toast.success("管理员已更新", password.length > 0 ? "密码已重置" : undefined);
			void queryClient.invalidateQueries({ queryKey: qk.admin.superAdmins() });
			onOpenChange(false);
		},
		onError: (error) => toast.apiError("更新管理员失败", error),
	});

	const isPending = createMutation.isPending || patchMutation.isPending;

	function submit(event: FormEvent<HTMLFormElement>) {
		event.preventDefault();
		const nextUsername = username.trim();
		const nextErrors: { username?: string; password?: string } = {};
		if (nextUsername.length === 0) nextErrors.username = "用户名不能为空";
		if (!row && password.length === 0) nextErrors.password = "新建账号必须设置密码";
		setErrors(nextErrors);
		if (nextErrors.username || nextErrors.password) return;

		if (row) {
			patchMutation.mutate({
				id: row.id,
				username: nextUsername,
				email: email.trim(),
				...(password.length > 0 ? { password } : {}),
			});
		} else {
			createMutation.mutate({ username: nextUsername, password, email: email.trim() });
		}
	}

	return (
		<FormSheet
			open={open}
			onOpenChange={onOpenChange}
			title={row ? `编辑管理员 ${row.username}` : "新建管理员"}
			description={
				row
					? "留空「新密码」表示不修改密码；改密后该账号需用新密码登录。"
					: "新账号立即获得完整管理端权限。"
			}
			width="lg"
			footer={
				<FormFooter
					formId="super-admin-form"
					isPending={isPending}
					submitLabel={row ? "保存" : "创建"}
					onCancel={() => onOpenChange(false)}
					hint={row ? `ID ${row.id}` : "用户名与密码为必填项"}
				/>
			}
		>
			<form id="super-admin-form" className="space-y-4" onSubmit={submit}>
				<FormGrid columns={2}>
					<Field label="用户名" htmlFor="super-admin-username" required error={errors.username}>
						<Input
							id="super-admin-username"
							autoComplete="off"
							value={username}
							onChange={(event) => setUsername(event.target.value)}
						/>
					</Field>
					<Field label="邮箱" htmlFor="super-admin-email">
						<Input
							id="super-admin-email"
							type="email"
							autoComplete="off"
							value={email}
							onChange={(event) => setEmail(event.target.value)}
						/>
					</Field>
				</FormGrid>
				<Field
					label={row ? "新密码（留空则不修改）" : "密码"}
					htmlFor="super-admin-password"
					required={!row}
					error={errors.password}
					hint="后端用 Argon2 哈希存储，创建后无法读取。"
				>
					<Input
						id="super-admin-password"
						type="password"
						autoComplete="new-password"
						value={password}
						onChange={(event) => setPassword(event.target.value)}
					/>
				</Field>
				{row ? (
					<div className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs">
						<ShieldOff className="mt-0.5 size-4 shrink-0 text-destructive" />
						<span>
							改密会立即生效；该账号已登录的会话在 JWT 过期前仍然有效，必要时请让其重新登录。
						</span>
					</div>
				) : null}
			</form>
		</FormSheet>
	);
}
