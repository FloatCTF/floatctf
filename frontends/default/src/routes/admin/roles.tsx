import { Checkbox, Stack, TextInput } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive, useTitle } from "ahooks";

import { adminApi } from "@/api";
import { GenericTable } from "@/components";
import { MyTruncate } from "@/components/Truncate";
import type { PermissionDto, RoleDto, UniResponse } from "@floatctf/sdk";
import { AdminRouteGuard } from "./route";

export const Route = createFileRoute("/admin/roles")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

/**
 * 平台角色管理（v2 新增）。
 *
 * 设计取舍（都指向同一件事：**不要把授权判定搬到前端**）：
 *
 * 1. 权限勾选框来自 `GET /api/admin/permissions` 的**后端目录**，不硬编码权限名 ——
 *    权限的权威在代码，前端本地再列一份就是「第二真相源」，加权限时必然漂移。
 * 2. 内置角色（`protected`）的权限集**不可改**：界面上直接禁用勾选并说明原因，
 *    而不是让用户改完再吃一个 409。
 * 3. 反提权（只能授予自己没有的权限）由**后端**判定并返回 403；界面只如实展示，
 *    不尝试在前端预判（前端预判会随权限变化而过期，还会给出误导性的"你可以"）。
 * 4. `grants_all` 角色的 `permissions` 通常为空（权限来自短路）——
 *    必须显式显示「全部权限」，否则看起来像个没有任何权限的空角色。
 */
const ALL_ACCESS_LABEL = "全部权限（含将来新增）";

function RouteComponent() {
	useTitle("Roles | FloatCTF");
	const subject = "Roles";

	// 权限目录：勾选框的唯一数据来源。
	const { data: permissionData } = useQuery({
		queryKey: ["admin", "permissions"],
		queryFn: () => adminApi.roles.listPermissions(),
	});
	const permissions: PermissionDto[] = permissionData?.data ?? [];

	const columns = [
		{ accessorKey: "id", header: "ID", field: "id", rowHeader: true },
		{
			accessorKey: "key",
			header: "标识",
			field: "key",
			label: "角色标识",
			sortBy: true,
		},
		{
			accessorKey: "name",
			header: "名称",
			field: "name",
			label: "名称",
			sortBy: true,
		},
		{
			accessorKey: "description",
			header: "说明",
			field: "description",
			label: "说明",
			renderCell: (row: RoleDto) => <MyTruncate value={row.description} />,
		},
		{
			accessorKey: "permissions",
			header: "权限",
			field: "permissions",
			label: "权限",
			renderCell: (row: RoleDto) => {
				// grants_all 角色的权限来自短路，行集合是空的 —— 必须显式说明，
				// 否则它在界面上看起来像个「什么都不能做」的角色。
				if (row.grants_all) {
					return (
						<span title="该角色拥有全部权限，且自动包含将来新增的权限">
							{ALL_ACCESS_LABEL}
						</span>
					);
				}
				const list = row.permissions ?? [];
				if (list.length === 0) return <span>—</span>;
				return <MyTruncate value={list.join(", ")} />;
			},
		},
		{
			accessorKey: "protected",
			header: "内置",
			field: "protected",
			label: "系统内置",
			renderCell: (row: RoleDto) => <span>{row.protected ? "是" : "否"}</span>,
		},
		{
			accessorKey: "user_count",
			header: "持有账号",
			field: "user_count",
			label: "持有账号数",
			sortBy: true,
		},
	];

	const mutationRole = useReactive<Partial<RoleDto>>({
		key: "",
		name: "",
		description: "",
		permissions: [],
		protected: false,
		grants_all: false,
	});

	const mutationColumns = [
		{
			header: "标识",
			field: "key",
			label: "角色标识（创建后不可改）",
			render: (
				<TextInput
					value={mutationRole.key ?? ""}
					disabled={Boolean(mutationRole.id)}
					placeholder="例如 event-manager"
					onChange={(e) => {
						mutationRole.key = e.target.value;
					}}
				/>
			),
		},
		{
			header: "名称",
			field: "name",
			label: "展示名称",
			render: (
				<TextInput
					value={mutationRole.name ?? ""}
					onChange={(e) => {
						mutationRole.name = e.target.value;
					}}
				/>
			),
		},
		{
			header: "说明",
			field: "description",
			label: "说明",
			render: (
				<TextInput
					value={mutationRole.description ?? ""}
					onChange={(e) => {
						mutationRole.description = e.target.value;
					}}
				/>
			),
		},
		{
			header: "权限",
			field: "permissions",
			label: "权限",
			render: (
				<Stack direction="vertical" gap="condensed">
					{mutationRole.grants_all ? (
						<span>{ALL_ACCESS_LABEL}</span>
					) : (
						permissions.map((permission) => {
							const selected = (mutationRole.permissions ?? []).includes(
								permission.key,
							);
							// 内置角色的权限集是引导不变式的一部分，后端会返回 409；
							// 这里直接禁用，避免让用户白改一遍。
							const locked = Boolean(mutationRole.protected);
							return (
								<Stack
									key={permission.key}
									direction="horizontal"
									align="start"
									gap="condensed"
								>
									<Checkbox
										checked={selected}
										disabled={locked}
										onChange={(e) => {
											const current = new Set(mutationRole.permissions ?? []);
											if (e.target.checked) current.add(permission.key);
											else current.delete(permission.key);
											mutationRole.permissions = [...current];
										}}
									/>
									<span>
										<code>{permission.key}</code>
										{permission.high_risk ? "（高危）" : ""}
										<br />
										<small>{permission.description}</small>
									</span>
								</Stack>
							);
						})
					)}
					{Boolean(mutationRole.protected) && (
						<span>
							内置角色的权限集不可修改（它属于平台引导不变式）
						</span>
					)}
				</Stack>
			),
		},
	];

	return (
		<GenericTable
			subject={subject}
			columns={columns}
			mutationColumns={mutationColumns}
			mutationData={mutationRole}
			defaultMutationData={{
				key: "",
				name: "",
				description: "",
				permissions: [],
				protected: false,
				grants_all: false,
			}}
			queryFn={adminApi.roles.fetch}
			// RBAC 的建角色请求体与 `Partial<RoleDto>` 不同构（有 key/name/permissions
			// 但语义是"创建"），显式适配，避免把 `id` / `user_count` 这类只读字段发上去。
			createFn={async (role: Partial<RoleDto>) =>
				adminApi.roles.create({
					key: role.key ?? "",
					name: role.name ?? "",
					description: role.description,
					permissions: role.permissions ?? [],
				})
			}
			// GenericTable 的签名是 `(Partial<T>) => ...`（id 由表格合并行数据提供），
			// 而 RBAC 端点以路径参数寻址，因此这里做一次显式适配。
			patchFn={async (role: Partial<RoleDto>) => {
				if (!role.id) throw new Error("roles.patch requires an id");
				return adminApi.roles.patch(role.id, {
					name: role.name,
					description: role.description,
					permissions: role.permissions,
				});
			}}
			// 同理：删除端点一次只接受一个 role_id，这里合并成 GenericTable 要的批量语义。
			removeFn={async (id_list: string[]) => {
				for (const id of id_list) {
					await adminApi.roles.remove(id);
				}
				return {
					code: 200,
					message: "ok",
					data: id_list.length,
				} as UniResponse<number>;
			}}
			disablePagination={true}
		/>
	);
}
