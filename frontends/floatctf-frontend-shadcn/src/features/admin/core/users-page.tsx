/**
 * 用户管理（`/admin/platform/users`）—— 列表 + Sheet 新建/编辑 + 批量删除。
 *
 * 铁律：`UsersDto` **包含 `password`（argon2 哈希）**，本页任何位置都不得渲染它；
 * 编辑表单也从不回填密码（留空 = 不修改）。
 */

import { useState, type ReactNode } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Pencil, Plus, RefreshCw, Trash2 } from "lucide-react";

import { call, callList } from "~/api/call";
import { useAppQueryClient, useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { useConfirm } from "~/components/app/confirm";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { Field, FormFooter, FormGrid, FormSheet } from "~/components/app/form";
import {
	PageBody,
	PageHeader,
	PaginationBar,
	SectionCard,
	Toolbar,
} from "~/components/app/page";
import { EmptyBlock, QueryState, TableSkeleton } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { UserCell } from "~/components/app/user-cell";
import { Button } from "~/components/ui/button";
import { DropdownMenuItem, DropdownMenuSeparator } from "~/components/ui/dropdown-menu";
import { Input } from "~/components/ui/input";
import { formatDateTime } from "~/lib/format";
import { useDocumentTitle } from "~/lib/hooks";
import type { QueryParams } from "@floatctf/sdk";
import type { Users } from "@floatctf/sdk/entity";

import { RowMenu, SearchInput, useAdminListState } from "./shared";

const SEARCH_KEYS = ["username", "nickname", "email"];

interface UserFormState {
	username: string;
	nickname: string;
	email: string;
	password: string;
}

type UserFormErrors = Partial<Record<keyof UserFormState, string>>;

const EMPTY_FORM: UserFormState = { username: "", nickname: "", email: "", password: "" };

function validate(form: UserFormState, mode: "create" | "edit"): UserFormErrors {
	const errors: UserFormErrors = {};
	const username = form.username.trim();
	if (!username) errors.username = "用户名不能为空";
	else if (username.length > 64) errors.username = "用户名最长 64 个字符";

	if (!form.nickname.trim()) errors.nickname = "昵称不能为空";

	const email = form.email.trim();
	if (email && !email.includes("@")) errors.email = "邮箱格式不正确";

	if (mode === "create") {
		if (!form.password) errors.password = "密码不能为空";
		else if (form.password.length < 8) errors.password = "密码至少 8 位";
	} else if (form.password && form.password.length < 8) {
		errors.password = "密码至少 8 位；留空表示不修改";
	}
	return errors;
}

export function AdminUsersPage(): ReactNode {
	useDocumentTitle("用户管理 · FloatCTF");
	const client = useClient();
	const queryClient = useAppQueryClient();
	const confirm = useConfirm();
	const list = useAdminListState(SEARCH_KEYS);

	const [selectedIds, setSelectedIds] = useState<string[]>([]);
	const [editing, setEditing] = useState<{ mode: "create" | "edit"; user?: Users } | null>(null);
	const [form, setForm] = useState<UserFormState>(EMPTY_FORM);
	const [errors, setErrors] = useState<UserFormErrors>({});

	const params: QueryParams = {
		page: list.page,
		limit: list.pageSize,
		...(list.filter ? { filter: list.filter } : {}),
	};

	const query = useQuery({
		queryKey: qk.admin.users(params),
		queryFn: () => callList<Users>(client.admin.users.fetch(params)),
	});

	function invalidate(): void {
		void queryClient.invalidateQueries({ queryKey: qk.admin.users() });
	}

	const save = useMutation({
		mutationFn: (input: { mode: "create" | "edit"; id?: string; body: UserFormState }) => {
			const payload = {
				username: input.body.username.trim(),
				nickname: input.body.nickname.trim(),
				email: input.body.email.trim(),
				...(input.body.password ? { password: input.body.password } : {}),
			};
			if (input.mode === "create") return call<Users>(client.admin.users.create(payload), "创建用户");
			// patch(Partial<Users>) 的 URL 用 `.id` 拼接，**必须**带 id。
			return call<Users>(client.admin.users.patch({ id: input.id ?? "", ...payload }), "更新用户");
		},
		onSuccess: (_data, input) => {
			toast.success(input.mode === "create" ? "用户已创建" : "用户已更新");
			invalidate();
			setEditing(null);
			setForm(EMPTY_FORM);
			setErrors({});
		},
		onError: (error) => toast.apiError("保存用户失败", error),
	});

	const remove = useMutation({
		mutationFn: (ids: string[]) => call<number>(client.admin.users.remove(ids), "删除用户"),
		onSuccess: (count) => {
			toast.success(`已删除 ${count} 个用户`, "其登录凭据同时失效。");
			invalidate();
			setSelectedIds([]);
		},
		onError: (error) => toast.apiError("删除用户失败", error),
	});

	function openCreate(): void {
		setForm(EMPTY_FORM);
		setErrors({});
		setEditing({ mode: "create" });
	}

	function openEdit(user: Users): void {
		// 密码永不回填：编辑时留空 = 不修改。
		setForm({ username: user.username, nickname: user.nickname, email: user.email, password: "" });
		setErrors({});
		setEditing({ mode: "edit", user });
	}

	function submit(): void {
		if (!editing) return;
		const nextErrors = validate(form, editing.mode);
		setErrors(nextErrors);
		if (Object.keys(nextErrors).length > 0) {
			toast.warning("请先修正表单中的错误");
			return;
		}
		save.mutate({ mode: editing.mode, id: editing.user?.id, body: form });
	}

	async function confirmRemoveOne(user: Users): Promise<void> {
		const ok = await confirm({
			title: `删除用户「${user.nickname || user.username}」？`,
			description: "该操作不可恢复。",
			consequences: [
				"账号立即无法登录，其选手端会话失效",
				"该用户的历史解题 / 提交记录因外键约束可能一并失败或被拒绝",
			],
			tone: "danger",
			confirmText: "删除",
			confirmPhrase: user.username,
		});
		if (ok) remove.mutate([user.id]);
	}

	async function confirmRemoveSelected(): Promise<void> {
		if (selectedIds.length === 0) {
			toast.warning("请先选择要删除的用户");
			return;
		}
		const ok = await confirm({
			title: `删除选中的 ${selectedIds.length} 个用户？`,
			description: "该操作不可恢复。",
			consequences: [
				`${selectedIds.length} 个账号立即无法登录`,
				"后端会逐个删除；已被其它数据引用的账号删除会失败并给出原因",
			],
			tone: "danger",
			confirmText: "删除",
			confirmPhrase: "delete",
		});
		if (ok) remove.mutate(selectedIds);
	}

	const columns: DataTableColumn<Users>[] = [
		{
			id: "user",
			header: "用户",
			cell: (row) => (
				// 只用 username / nickname / avatar —— 绝不渲染 password。
				<UserCell username={row.username} nickname={row.nickname} avatar={row.avatar} />
			),
			sortValue: (row) => row.username,
		},
		{
			id: "email",
			header: "邮箱",
			cell: (row) =>
				row.email ? (
					<span className="text-sm">{row.email}</span>
				) : (
					<span className="text-xs text-muted-foreground">—</span>
				),
			sortValue: (row) => row.email,
			hideBelow: "sm",
		},
		{
			id: "created_at",
			header: "注册时间",
			cell: (row) => (
				<span className="tnum text-xs text-muted-foreground">{formatDateTime(row.created_at)}</span>
			),
			sortValue: (row) => Date.parse(row.created_at),
			hideBelow: "md",
		},
		{
			id: "updated_at",
			header: "最近更新",
			cell: (row) => (
				<span className="tnum text-xs text-muted-foreground">{formatDateTime(row.updated_at)}</span>
			),
			sortValue: (row) => Date.parse(row.updated_at),
			hideBelow: "lg",
		},
	];

	return (
		<PageBody>
			<PageHeader
				title="用户管理"
				description="平台账号的增删改查。密码字段是 argon2 哈希，前后端都不展示。"
				actions={
					<Toolbar>
						<Button variant="outline" onClick={() => query.refetch()} disabled={query.isFetching}>
							<RefreshCw className={query.isFetching ? "animate-spin" : undefined} />
							刷新
						</Button>
						<Button onClick={openCreate}>
							<Plus /> 新建用户
						</Button>
					</Toolbar>
				}
			/>

			<SectionCard
				title="账号列表"
				description="按搜索词在用户名 / 昵称 / 邮箱上做服务端 OR 匹配。"
				actions={
					<Toolbar>
						<SearchInput
							value={list.search}
							onChange={list.setSearch}
							placeholder="搜索用户名 / 昵称 / 邮箱"
						/>
						{selectedIds.length > 0 ? (
							<Button variant="destructive" size="sm" onClick={() => void confirmRemoveSelected()}>
								<Trash2 /> 删除选中（{selectedIds.length}）
							</Button>
						) : null}
					</Toolbar>
				}
				contentClassName="p-0"
				footer={
					// 数据到位前不渲染分页条，避免出现「共 0 条」这种与事实不符的文案。
					query.data ? (
						<PaginationBar
							page={list.page}
							pageSize={list.pageSize}
							total={query.data.meta?.total ?? 0}
							onPageChange={list.setPage}
							onPageSizeChange={list.setPageSize}
						/>
					) : null
				}
			>
				<QueryState
					query={query}
					skeleton={<TableSkeleton rows={6} columns={4} />}
					errorTitle="加载用户列表失败"
					isEmpty={(result) => result.items.length === 0}
					empty={
						list.filter ? (
							<EmptyBlock
								variant="filtered"
								title="没有匹配的用户"
								description={`没有用户名 / 昵称 / 邮箱匹配「${list.search}」的账号。`}
							/>
						) : (
							<EmptyBlock
								title="还没有用户"
								description="创建第一个账号，或让选手自行注册。"
								action={
									<Button size="sm" onClick={openCreate}>
										<Plus /> 新建用户
									</Button>
								}
							/>
						)
					}
				>
					{(result) => (
						<DataTable
							data={result.items}
							getRowId={(row) => row.id}
							columns={columns}
							selectable
							selectedIds={selectedIds}
							onSelectionChange={setSelectedIds}
							mobileCard={(row) => (
								<div className="space-y-1.5">
									<UserCell username={row.username} nickname={row.nickname} avatar={row.avatar} />
									<p className="text-xs text-muted-foreground">{row.email || "—"}</p>
									<p className="tnum text-xs text-muted-foreground">
										注册 {formatDateTime(row.created_at)}
									</p>
									<div className="flex gap-2 pt-1">
										<Button variant="outline" size="sm" onClick={() => openEdit(row)}>
											<Pencil /> 编辑
										</Button>
										<Button
											variant="destructive"
											size="sm"
											onClick={() => void confirmRemoveOne(row)}
										>
											<Trash2 /> 删除
										</Button>
									</div>
								</div>
							)}
							rowActions={(row) => (
								<RowMenu label={`用户 ${row.username} 的操作`}>
									<DropdownMenuItem onSelect={() => openEdit(row)}>
										<Pencil /> 编辑
									</DropdownMenuItem>
									<DropdownMenuSeparator />
									<DropdownMenuItem
										variant="destructive"
										onSelect={() => void confirmRemoveOne(row)}
									>
										<Trash2 /> 删除
									</DropdownMenuItem>
								</RowMenu>
							)}
						/>
					)}
				</QueryState>
			</SectionCard>

			<FormSheet
				open={editing !== null}
				onOpenChange={(open) => {
					if (!open) setEditing(null);
				}}
				title={editing?.mode === "edit" ? "编辑用户" : "新建用户"}
				description={
					editing?.mode === "edit"
						? "留空密码表示不修改；用户名 / 昵称 / 邮箱修改后立即生效。"
						: "创建一个平台账号。用户名为登录凭据，注册后仍可修改。"
				}
				width="md"
				footer={
					<FormFooter
						onCancel={() => setEditing(null)}
						submitLabel={editing?.mode === "edit" ? "保存修改" : "创建用户"}
						isPending={save.isPending}
						formId="admin-user-form"
					/>
				}
			>
				<form
					id="admin-user-form"
					className="space-y-4"
					onSubmit={(event) => {
						event.preventDefault();
						submit();
					}}
				>
					<FormGrid columns={1}>
						<Field label="用户名" htmlFor="user-username" required error={errors.username} hint="最长 64 个字符">
							<Input
								id="user-username"
								autoComplete="off"
								value={form.username}
								onChange={(event) => setForm({ ...form, username: event.target.value })}
							/>
						</Field>
						<Field label="昵称" htmlFor="user-nickname" required error={errors.nickname}>
							<Input
								id="user-nickname"
								autoComplete="off"
								value={form.nickname}
								onChange={(event) => setForm({ ...form, nickname: event.target.value })}
							/>
						</Field>
						<Field label="邮箱" htmlFor="user-email" error={errors.email} hint="可留空；用于找回密码。">
							<Input
								id="user-email"
								type="email"
								autoComplete="off"
								value={form.email}
								onChange={(event) => setForm({ ...form, email: event.target.value })}
							/>
						</Field>
						<Field
							label={editing?.mode === "edit" ? "新密码" : "密码"}
							htmlFor="user-password"
							required={editing?.mode !== "edit"}
							error={errors.password}
							hint={editing?.mode === "edit" ? "留空表示不修改；填写则至少 8 位。" : "至少 8 位。"}
						>
							<Input
								id="user-password"
								type="password"
								autoComplete="new-password"
								value={form.password}
								onChange={(event) => setForm({ ...form, password: event.target.value })}
							/>
						</Field>
					</FormGrid>
				</form>
			</FormSheet>
		</PageBody>
	);
}
