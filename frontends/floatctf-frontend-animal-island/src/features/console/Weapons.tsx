/**
 * 武器库管理。
 *
 * 「武器」是 AWD 场景下战队可购买的攻防工具，因此除了名称/分类，最重要的是
 * **附件文件**：`has_file` 与 `file_url` 决定选手能否真的下载到它。
 * 上传按钮只在该武器缺少文件时才强调，避免管理员反复重传。
 */
import { useState } from "react";
import {
	Button,
	Input,
	Modal,
	Notification,
	Switch,
	Tag,
	Upload,
	type TableColumn,
} from "animal-island-ui";
import { DownloadIcon, EditIcon, PlusIcon, TrashIcon, UploadIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { Can, useCan } from "~/app/permissions";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { ResourcePage } from "~/components/resource";
import type { Weapons } from "@floatctf/sdk/entity";

export function ConsoleWeapons() {
	const runtime = useRuntime();
	const confirm = useConfirm();
	const [editing, setEditing] = useState<Partial<Weapons> | null>(null);

	const query = useUniQuery(
		["console", "weapons"],
		() => runtime.client.admin.weapons.fetch(),
		"加载武器库",
	);
	const save = useUniMutation(
		(vars: Partial<Weapons>) =>
			vars.id ? runtime.client.admin.weapons.patch(vars) : runtime.client.admin.weapons.create(vars),
		"保存武器",
	);
	const remove = useUniMutation(
		(vars: string[]) => runtime.client.admin.weapons.remove(vars),
		"删除武器",
	);
	const upload = useUniMutation(
		(vars: { id: string; file: File }) => runtime.client.admin.weapons.upload(vars.id, vars.file),
		"上传武器文件",
	);

	// 「操作」列整列随权限显隐：只读用户看不到写入口，

	// 也不该看到一个永远为空的操作列（见下面的 columns 过滤）。

	const can = useCan();

	const columns: TableColumn<Weapons>[] = [
		{ title: "名称", dataIndex: "name", width: 220 },
		{
			title: "分类",
			dataIndex: "category",
			width: 140,
			render: (v) => (
				<Tag size="small" variant="outlined">
					{String(v)}
				</Tag>
			),
		},
		{
			title: "文件",
			dataIndex: "has_file",
			width: 100,
			render: (v) =>
				v ? (
					<Tag size="small" variant="soft" color="app-green">
						有
					</Tag>
				) : (
					<Tag size="small" variant="soft" color="app-red">
						缺
					</Tag>
				),
		},
		{ title: "下载次数", dataIndex: "download_count", width: 100, align: "right" },
		{
			title: "操作",
			width: 260,
			align: "right",
			render: (_v, row) => (
				<Can permission="weapons.manage">
				<div className="ai-row" style={{ justifyContent: "flex-end", gap: 6 }}>
					<Upload
						showUploadList={false}
						customRequest={({ file, onSuccess, onError }) => {
							upload
								.mutateAsync({ id: row.id, file })
								.then((r) => {
									Notification.success({ message: "文件已上传" });
									onSuccess(r);
									void query.refetch();
								})
								.catch(onError);
						}}
					>
						<Button
							size="small"
							type={row.has_file ? "text" : "primary"}
							icon={<UploadIcon size={14} />}
						>
							{row.has_file ? "替换文件" : "上传文件"}
						</Button>
					</Upload>
					{row.file_url ? (
						<Button
							size="small"
							type="text"
							icon={<DownloadIcon size={14} />}
							onClick={() => window.open(row.file_url ?? "", "_blank", "noopener")}
						>
							下载
						</Button>
					) : null}
					<Button
						size="small"
						type="text"
						icon={<EditIcon size={14} />}
						onClick={() => setEditing({ ...row })}
					>
						编辑
					</Button>
					<Button
						size="small"
						type="text"
						danger
						icon={<TrashIcon size={14} />}
						onClick={async () => {
							const ok = await confirm({
								title: `删除武器「${row.name}」`,
								consequence:
									"武器与它的附件文件都会从平台移除。**已经购买/依赖它的 AWD 战队会失去这个工具**，且无法恢复。",
								okText: "删除",
								danger: true,
								confirmPhrase: row.name,
							});
							if (!ok) return;
							await remove.mutateAsync([row.id]);
							void query.refetch();
						}}
					>
						删除
					</Button>
				</div>
				</Can>
			),
		},
	];

	return (
		<>
			<PageHeader
				title="武器库"
				description="AWD 战队可购买的攻防工具。没有文件的武器选手拿不到，因此「文件」一列值得盯着。"
				extra={
					<Can permission="weapons.manage">
						<Button
							type="primary"
							icon={<PlusIcon size={16} />}
							onClick={() => setEditing({ name: "", category: "", description: "" })}
						>
							新建武器
						</Button>
					</Can>
				}
			/>

			<ResourcePage<Weapons>
				columns={can("weapons.manage") ? columns : columns.filter((c) => c.title !== "操作")}
				rows={query.data}
				isLoading={query.isLoading}
				error={query.error}
				refetch={() => void query.refetch()}
				rowKey="id"
				emptyTitle="武器库是空的"
				emptyDescription="新建一个武器，然后上传它的附件文件。"
				disablePagination
			/>

			<Modal
				open={editing !== null}
				title={editing?.id ? "编辑武器" : "新建武器"}
				typewriter={false}
				onClose={() => setEditing(null)}
				footer={
					<>
						<Button onClick={() => setEditing(null)}>取消</Button>
						<Button
							type="primary"
							disabled={!editing?.name?.trim()}
							loading={save.isPending}
							onClick={async () => {
								if (!editing) return;
								await save.mutateAsync({ ...editing, name: editing.name?.trim() });
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
				<div className="ai-col" style={{ gap: 12 }}>
					<label className="ai-col" style={{ gap: 6 }}>
						<span>名称</span>
						<Input
							value={editing?.name ?? ""}
							onChange={(e) => setEditing({ ...editing, name: e.target.value })}
						/>
					</label>
					<label className="ai-col" style={{ gap: 6 }}>
						<span>分类</span>
						<Input
							value={editing?.category ?? ""}
							onChange={(e) => setEditing({ ...editing, category: e.target.value })}
						/>
					</label>
					<div className="ai-row" style={{ gap: 8 }}>
						<span className="ai-muted">
							文件在保存后通过列表里的「上传文件」按钮上传。
						</span>
					</div>
					{editing?.id ? (
						<label className="ai-row" style={{ gap: 8 }}>
							<Switch
								checked={Boolean(editing.has_file)}
								onChange={() => {
									/* has_file 由上传决定，不手工改；此开关仅展示状态 */
								}}
								disabled
							/>
							<span className="ai-muted">文件状态（由上传决定，不可手工修改）</span>
						</label>
					) : null}
				</div>
			</Modal>
		</>
	);
}
