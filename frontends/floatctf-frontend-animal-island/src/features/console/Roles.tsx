/**
 * 角色与权限。
 *
 * 两条与后端契约对齐的硬规则，界面必须如实表达：
 *
 * 1. **权限来自后端目录**（`GET /api/admin/permissions`），前端不维护第二份。
 *    前端再列一份权限表，加权限时必然漂移。
 * 2. **内置角色的权限集不可改**（`protected`）：界面直接禁用勾选并说明原因，
 *    而不是让用户改完再吃一个 409。
 *
 * 高危权限（`high_risk`）在选项里显式标红 —— 授予它们之前应该有人停一下。
 */
import { useState } from "react";
import {
	Button,
	Checkbox,
	Divider,
	Input,
	Modal,
	Notification,
	Tag,
	Tooltip,
	type TableColumn,
} from "animal-island-ui";
import { EditIcon, KeyIcon, LockIcon, PlusIcon, ShieldIcon, TrashIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { Can, useCan } from "~/app/permissions";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { ResourcePage } from "~/components/resource";
import type { PermissionDto, RoleDto } from "@floatctf/sdk";

type Draft = {
	id?: string;
	key: string;
	name: string;
	description: string;
	permissions: string[];
	protected?: boolean;
	grants_all?: boolean;
};

export function ConsoleRoles() {
	const runtime = useRuntime();
	const confirm = useConfirm();
	const [editing, setEditing] = useState<Draft | null>(null);

	const roles = useUniQuery(
		["console", "roles"],
		() => runtime.client.admin.roles.fetch(),
		"加载角色",
	);
	const permissions = useUniQuery(
		["console", "permissions"],
		() => runtime.client.admin.roles.listPermissions(),
		"加载权限目录",
	);

	const save = useUniMutation(
		(vars: Draft) =>
			vars.id
				? runtime.client.admin.roles.patch(vars.id, {
						name: vars.name,
						description: vars.description,
						permissions: vars.permissions,
					})
				: runtime.client.admin.roles.create({
						key: vars.key,
						name: vars.name,
						description: vars.description,
						permissions: vars.permissions,
					}),
		"保存角色",
	);
	const remove = useUniMutation(
		(vars: string) => runtime.client.admin.roles.remove(vars),
		"删除角色",
	);

	const permByKey = new Map<string, PermissionDto>(
		(permissions.data ?? []).map((p) => [p.key, p]),
	);

	// 「操作」列整列随权限显隐：只读用户看不到写入口，

	// 也不该看到一个永远为空的操作列（见下面的 columns 过滤）。

	const can = useCan();

	const columns: TableColumn<RoleDto>[] = [
		{ title: "名称", dataIndex: "name", width: 200 },
		{
			title: "标识",
			dataIndex: "key",
			width: 160,
			render: (v) => (
				<Tag size="small" variant="outlined">
					{String(v)}
				</Tag>
			),
		},
		{
			title: "权限",
			dataIndex: "permissions",
			width: 300,
			render: (_v, row) =>
				row.grants_all ? (
					<Tag size="small" variant="solid" color="app-red">
						<ShieldIcon size={12} /> 全部权限（含将来新增）
					</Tag>
				) : (
					<span className="ai-muted" style={{ fontSize: 12 }}>
						{(row.permissions ?? []).length} 项
						{row.permissions?.length
							? `：${row.permissions.slice(0, 3).join("、")}${row.permissions.length > 3 ? "…" : ""}`
							: ""}
					</span>
				),
		},
		{
			title: "内置",
			dataIndex: "protected",
			width: 90,
			render: (v) =>
				v ? (
					<Tag size="small" variant="soft" color="brown">
						<LockIcon size={12} /> 内置
					</Tag>
				) : (
					<span className="ai-muted">—</span>
				),
		},
		{ title: "持有账号", dataIndex: "user_count", width: 100, align: "right" },
		{
			title: "操作",
			width: 170,
			align: "right",
			render: (_v, row) => (
				<div className="ai-row" style={{ justifyContent: "flex-end", gap: 6 }}>
					<Button
						size="small"
						type="text"
						icon={<EditIcon size={14} />}
						onClick={() =>
							setEditing({
								id: row.id,
								key: row.key,
								name: row.name,
								description: row.description,
								permissions: [...(row.permissions ?? [])],
								protected: row.protected,
								grants_all: row.grants_all,
							})
						}
					>
						编辑
					</Button>
					<Button
						size="small"
						type="text"
						danger
						icon={<TrashIcon size={14} />}
						disabled={row.protected}
						onClick={async () => {
							const ok = await confirm({
								title: `删除角色「${row.name}」`,
								consequence:
									"角色会被删除。**当前持有它的账号会立刻失去由此角色带来的全部权限**（后端每请求重新解析权限）。如果还有账号持有，后端会拒绝删除 —— 请先解除授予。",
								okText: "删除角色",
								danger: true,
								confirmPhrase: row.key,
							});
							if (!ok) return;
							await remove.mutateAsync(row.id);
							Notification.success({ message: "已删除" });
							void roles.refetch();
						}}
					>
						删除
					</Button>
				</div>
			),
		},
	];

	return (
		<>
			<PageHeader
				title="角色与权限"
				description="平台角色决定账号能看到与能做什么。权限目录来自后端，前端不维护第二份。"
				extra={
					<Can permission="roles.manage">
						<Button
							type="primary"
							icon={<PlusIcon size={16} />}
							onClick={() =>
								setEditing({ key: "", name: "", description: "", permissions: [] })
							}
						>
							新建角色
						</Button>
					</Can>
				}
			/>

			<ResourcePage<RoleDto>
				columns={can("roles.manage") ? columns : columns.filter((c) => c.title !== "操作")}
				rows={roles.data}
				isLoading={roles.isLoading || permissions.isLoading}
				error={roles.error ?? permissions.error}
				refetch={() => {
					void roles.refetch();
					void permissions.refetch();
				}}
				rowKey="id"
				emptyTitle="还没有角色"
				emptyDescription="内置的 super-admin 由平台引导创建；这里可以创建更细的角色。"
				disablePagination
			/>

			<Modal
				open={editing !== null}
				title={editing?.id ? `编辑角色「${editing.name}」` : "新建角色"}
				typewriter={false}
				width={680}
				onClose={() => setEditing(null)}
				footer={
					<>
						<Button onClick={() => setEditing(null)}>取消</Button>
						<Button
							type="primary"
							disabled={!editing?.name?.trim() || !editing?.key?.trim()}
							loading={save.isPending}
							onClick={async () => {
								if (!editing) return;
								await save.mutateAsync(editing);
								Notification.success({ message: editing.id ? "已更新" : "已创建" });
								setEditing(null);
								void roles.refetch();
							}}
						>
							保存
						</Button>
					</>
				}
			>
				{editing ? (
					<div className="ai-col" style={{ gap: 14 }}>
						<label className="ai-col" style={{ gap: 6 }}>
							<span>角色标识（创建后不可改）</span>
							<Input
								value={editing.key}
								disabled={Boolean(editing.id)}
								placeholder="例如 event-manager"
								onChange={(e) => setEditing({ ...editing, key: e.target.value })}
							/>
						</label>
						<label className="ai-col" style={{ gap: 6 }}>
							<span>名称</span>
							<Input
								value={editing.name}
								onChange={(e) => setEditing({ ...editing, name: e.target.value })}
							/>
						</label>
						<label className="ai-col" style={{ gap: 6 }}>
							<span>说明</span>
							<Input
								value={editing.description}
								onChange={(e) => setEditing({ ...editing, description: e.target.value })}
							/>
						</label>

						<Divider />

						<div className="ai-col" style={{ gap: 8 }}>
							<span className="ai-strong">权限</span>

							{editing.grants_all ? (
								<span className="ai-muted">
									这是全权角色：它拥有全部权限，**且自动包含将来新增的权限**。
									因此它的权限集合在数据上是空的 —— 这不是"没权限"。
								</span>
							) : editing.protected ? (
								<div className="ai-row" style={{ gap: 8 }}>
									<LockIcon size={16} />
									<span className="ai-muted">
										内置角色的权限集属于平台引导不变式，**不可修改**。后端也会拒绝。
									</span>
								</div>
							) : null}

							<Checkbox
								direction="vertical"
								disabled={Boolean(editing.protected) || Boolean(editing.grants_all)}
								value={editing.permissions}
								options={(permissions.data ?? []).map((p) => ({
									value: p.key,
									label: (
										<div className="ai-col" style={{ gap: 2, alignItems: "flex-start" }}>
											<span className="ai-row" style={{ gap: 6 }}>
												<span className="ai-strong">{p.key}</span>
												{p.high_risk ? (
													<Tooltip title="高危权限：授予前请确认必要性与影响面">
														<Tag size="small" variant="solid" color="app-red">
															高危
														</Tag>
													</Tooltip>
												) : null}
												<Tag size="small" variant="outlined">
													{p.domain}
												</Tag>
											</span>
											<span className="ai-muted" style={{ fontSize: 12 }}>
												{p.description}
											</span>
										</div>
									),
								}))}
								onChange={(values) =>
									setEditing({ ...editing, permissions: values.map(String) })
								}
							/>
						</div>

						{editing.permissions.length > 0 && !editing.grants_all ? (
							<div className="ai-row" style={{ gap: 8 }}>
								<KeyIcon size={16} />
								<span className="ai-muted">
									已选 {editing.permissions.length} 项
									{editing.permissions.some((k) => permByKey.get(k)?.high_risk)
										? "，其中包含高危权限"
										: ""}
								</span>
							</div>
						) : null}
					</div>
				) : null}
			</Modal>
		</>
	);
}
