/**
 * 用户管理。
 *
 * 安全不变量：**响应 DTO 里没有 password 字段**（v2 起后端根本不返回），
 * 因此这里不可能"顺手显示出来"；编辑时密码框永远为空、只写不读。
 */
import { useState } from "react";
import { useNavigate } from "react-router";
import { Button, Input, Modal, Notification, Tag, type TableColumn } from "animal-island-ui";
import { EditIcon, KeyIcon, PlusIcon, TrashIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { Can, useCan } from "~/app/permissions";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { ResourcePage } from "~/components/resource";
import { formatTime } from "~/lib/event";
import type { UserDto } from "@floatctf/sdk";

type Draft = Partial<UserDto> & { password?: string };

export function ConsoleUsers() {
	const runtime = useRuntime();
	const navigate = useNavigate();
	const confirm = useConfirm();
	const [editing, setEditing] = useState<Draft | null>(null);

	const query = useUniQuery(
		["console", "users"],
		() => runtime.client.admin.users.fetch(),
		"加载用户",
	);
	const save = useUniMutation(
		(vars: Draft) =>
			vars.id ? runtime.client.admin.users.patch(vars) : runtime.client.admin.users.create(vars),
		"保存用户",
	);
	const remove = useUniMutation(
		(vars: string[]) => runtime.client.admin.users.remove(vars),
		"删除用户",
	);

	// 「操作」列整列随权限显隐：只读用户看不到写入口，

	// 也不该看到一个永远为空的操作列（见下面的 columns 过滤）。

	const can = useCan();

	const columns: TableColumn<UserDto>[] = [
		{ title: "用户名", dataIndex: "username", width: 170 },
		{ title: "昵称", dataIndex: "nickname", width: 170 },
		{ title: "邮箱", dataIndex: "email", width: 230 },
		{
			title: "注册时间",
			dataIndex: "created_at",
			width: 170,
			render: (v) => <span className="ai-muted">{formatTime(String(v))}</span>,
		},
		{
			title: "操作",
			width: 230,
			align: "right",
			render: (_v, row) => (
				<div className="ai-row" style={{ justifyContent: "flex-end", gap: 6 }}>
					<Button
						size="small"
						icon={<KeyIcon size={14} />}
						onClick={() => void navigate(`/console/users/${row.id}`)}
					>
						角色
					</Button>
					<Can permission="users.manage">
						<Button
							size="small"
							type="text"
							icon={<EditIcon size={14} />}
							onClick={() => setEditing({ ...row, password: "" })}
						>
							编辑
						</Button>
					</Can>
					<Can permission="users.manage">
					<Button
						size="small"
						type="text"
						danger
						icon={<TrashIcon size={14} />}
						onClick={async () => {
							const ok = await confirm({
								title: `删除账号「${row.username}」`,
								consequence:
									"账号会被删除。该账号的登录能力立即失效；**它产生的审计日志会被保留**（审计主体置空），但解题、题解与讨论内容会随账号一并处理。无法恢复。",
								okText: "删除账号",
								danger: true,
								confirmPhrase: row.username,
							});
							if (!ok) return;
							await remove.mutateAsync([row.id]);
							void query.refetch();
						}}
					>
						删除
					</Button>
					</Can>
				</div>
			),
		},
	];

	return (
		<>
			<PageHeader
				title="用户"
				description="平台账号。角色与权限在账号详情里授予。"
				extra={
					<Can permission="users.manage">
						<Button
							type="primary"
							icon={<PlusIcon size={16} />}
							onClick={() =>
								setEditing({ username: "", nickname: "", email: "", password: "" })
							}
						>
							新建用户
						</Button>
					</Can>
				}
			/>

			<ResourcePage<UserDto>
				columns={can("users.manage") ? columns : columns.filter((c) => c.title !== "操作")}
				rows={query.data}
				isLoading={query.isLoading}
				error={query.error}
				refetch={() => void query.refetch()}
				rowKey="id"
				emptyTitle="还没有用户"
				disablePagination
			/>

			<Modal
				open={editing !== null}
				title={editing?.id ? `编辑 ${editing.username}` : "新建用户"}
				typewriter={false}
				onClose={() => setEditing(null)}
				footer={
					<>
						<Button onClick={() => setEditing(null)}>取消</Button>
						<Button
							type="primary"
							disabled={!editing?.username?.trim()}
							loading={save.isPending}
							onClick={async () => {
								if (!editing) return;
								// 空密码表示"不改"，不要把它当成设为空串提交上去。
								const payload: Draft = { ...editing };
								if (!payload.password) delete payload.password;
								await save.mutateAsync(payload);
								Notification.success({ message: editing.id ? "已更新" : "已创建" });
								setEditing(null);
								void query.refetch();
							}}
						>
							保存
						</Button>
					</>
				}
			>
				{editing ? (
					<div className="ai-col" style={{ gap: 12 }}>
						<label className="ai-col" style={{ gap: 6 }}>
							<span>用户名</span>
							<Input
								value={editing.username ?? ""}
								disabled={Boolean(editing.id)}
								onChange={(e) => setEditing({ ...editing, username: e.target.value })}
							/>
							{editing.id ? (
								<span className="ai-muted" style={{ fontSize: 12 }}>
									用户名是登录标识，创建后不可修改。
								</span>
							) : null}
						</label>
						<label className="ai-col" style={{ gap: 6 }}>
							<span>昵称</span>
							<Input
								value={editing.nickname ?? ""}
								onChange={(e) => setEditing({ ...editing, nickname: e.target.value })}
							/>
						</label>
						<label className="ai-col" style={{ gap: 6 }}>
							<span>邮箱</span>
							<Input
								value={editing.email ?? ""}
								onChange={(e) => setEditing({ ...editing, email: e.target.value })}
							/>
						</label>
						<label className="ai-col" style={{ gap: 6 }}>
							<span>{editing.id ? "重置密码（留空不改）" : "初始密码"}</span>
							<Input
								type="password"
								value={editing.password ?? ""}
								onChange={(e) => setEditing({ ...editing, password: e.target.value })}
							/>
							<span className="ai-muted" style={{ fontSize: 12 }}>
								密码只写不读：后端响应里根本没有该字段，因此这里也永远无法回显。
							</span>
						</label>
						{editing.id ? (
							<Tag size="small" variant="soft" color="app-yellow">
								改密会递增该账号的令牌版本，吊销其所有已登录会话
							</Tag>
						) : null}
					</div>
				) : null}
			</Modal>
		</>
	);
}
