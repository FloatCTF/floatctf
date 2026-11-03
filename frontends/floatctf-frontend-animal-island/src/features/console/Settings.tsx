/**
 * 动态设置。
 *
 * `protected` 设置是平台**引导不变式**的一部分（例如前端激活开关），
 * 后端会拒绝修改；界面因此直接把该行标出来并禁掉编辑入口，
 * 而不是让管理员改完再吃一个错误。
 *
 * `resolved_value` 是后端算出来的**生效值**，与 `value`（原始配置）可能不同 ——
 * 两列都显示，否则"我明明改了却没生效"会变成一个查不出原因的问题。
 */
import { useState } from "react";
import {
	Button,
	Input,
	Modal,
	Notification,
	Select,
	Switch,
	Tag,
	type TableColumn,
} from "animal-island-ui";
import { EditIcon, LockIcon, PlusIcon, TrashIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { Can, useCan } from "~/app/permissions";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { ResourcePage } from "~/components/resource";
import { formatTime } from "~/lib/event";
import { SettingValueType } from "@floatctf/sdk/entity";
import type { SettingsDto } from "@floatctf/sdk";

type Draft = Partial<SettingsDto> & { type?: SettingValueType };

export function ConsoleSettings() {
	const runtime = useRuntime();
	const confirm = useConfirm();
	const [editing, setEditing] = useState<Draft | null>(null);

	const query = useUniQuery(
		["console", "settings"],
		() => runtime.client.admin.settings.fetch(),
		"加载设置",
	);
	const save = useUniMutation(
		(vars: Draft) =>
			vars.id
				? runtime.client.admin.settings.patch(vars)
				: runtime.client.admin.settings.create(vars),
		"保存设置",
	);
	const remove = useUniMutation(
		(vars: string[]) => runtime.client.admin.settings.remove(vars),
		"删除设置",
	);

	// 「操作」列整列随权限显隐：只读用户看不到写入口，

	// 也不该看到一个永远为空的操作列（见下面的 columns 过滤）。

	const can = useCan();

	const columns: TableColumn<SettingsDto>[] = [
		{ title: "键", dataIndex: "key", width: 220 },
		{
			title: "值",
			dataIndex: "value",
			width: 180,
			render: (v) => <span className="ai-mono">{String(v)}</span>,
		},
		{
			title: "生效值",
			dataIndex: "resolved_value",
			width: 180,
			render: (v) => <span className="ai-mono">{String(v)}</span>,
		},
		{
			title: "类型",
			dataIndex: "type",
			width: 100,
			render: (v) => (
				<Tag size="small" variant="outlined">
					{String(v)}
				</Tag>
			),
		},
		{ title: "说明", dataIndex: "description", width: 220 },
		{
			title: "保护",
			dataIndex: "protected",
			width: 90,
			render: (v) =>
				v ? (
					<Tag size="small" variant="soft" color="brown">
						<LockIcon size={12} /> 受保护
					</Tag>
				) : (
					<span className="ai-muted">—</span>
				),
		},
		{
			title: "更新时间",
			dataIndex: "updated_at",
			width: 160,
			render: (v) => <span className="ai-muted">{formatTime(String(v))}</span>,
		},
		{
			title: "操作",
			width: 170,
			align: "right",
			render: (_v, row) => (
				<Can permission="settings.manage">
				<div className="ai-row" style={{ justifyContent: "flex-end", gap: 6 }}>
					<Button
						size="small"
						type="text"
						icon={<EditIcon size={14} />}
						disabled={row.protected}
						onClick={() => setEditing({ ...row })}
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
								title: `删除设置「${row.key}」`,
								consequence:
									"删除后该配置会回落到代码里的默认值（或变成未配置）。**依赖它的功能行为会立刻改变**，例如前端激活开关、判题超时。",
								okText: "删除",
								danger: true,
								confirmPhrase: row.key,
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
				title="动态设置"
				description="运行期配置。受保护的项属于平台引导不变式，后端会拒绝修改。"
				extra={
					<Can permission="settings.manage">
						<Button
							type="primary"
							icon={<PlusIcon size={16} />}
							onClick={() =>
								setEditing({
									key: "",
									value: "",
									type: SettingValueType.String,
									description: "",
								})
							}
						>
							新建设置
						</Button>
					</Can>
				}
			/>

			<ResourcePage<SettingsDto>
				columns={can("settings.manage") ? columns : columns.filter((c) => c.title !== "操作")}
				rows={query.data}
				isLoading={query.isLoading}
				error={query.error}
				refetch={() => void query.refetch()}
				rowKey="id"
				emptyTitle="还没有设置"
				disablePagination
			/>

			<Modal
				open={editing !== null}
				title={editing?.id ? `编辑 ${editing.key}` : "新建设置"}
				typewriter={false}
				onClose={() => setEditing(null)}
				footer={
					<>
						<Button onClick={() => setEditing(null)}>取消</Button>
						<Button
							type="primary"
							disabled={!editing?.key?.trim()}
							loading={save.isPending}
							onClick={async () => {
								if (!editing) return;
								await save.mutateAsync({ ...editing, key: editing.key?.trim() });
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
							<span>键</span>
							<Input
								value={editing.key ?? ""}
								disabled={Boolean(editing.id)}
								placeholder="例如 FRONTEND_ACTIVE"
								onChange={(e) => setEditing({ ...editing, key: e.target.value })}
							/>
						</label>
						<label className="ai-col" style={{ gap: 6 }}>
							<span>值</span>
							<Input
								value={editing.value ?? ""}
								onChange={(e) => setEditing({ ...editing, value: e.target.value })}
							/>
						</label>
						<div className="ai-col" style={{ gap: 6 }}>
							<span>类型</span>
							<Select
								value={editing.type ?? SettingValueType.String}
								onChange={(v) => setEditing({ ...editing, type: v as SettingValueType })}
								options={[
									{ key: SettingValueType.String, label: "字符串" },
									{ key: SettingValueType.Integer, label: "整数" },
									{ key: SettingValueType.Float, label: "浮点" },
									{ key: SettingValueType.Boolean, label: "布尔" },
								]}
							/>
						</div>
						<label className="ai-col" style={{ gap: 6 }}>
							<span>说明</span>
							<Input
								value={editing.description ?? ""}
								onChange={(e) => setEditing({ ...editing, description: e.target.value })}
							/>
						</label>
						<div className="ai-row" style={{ gap: 8 }}>
							<Switch
								checked={Boolean(editing.protected)}
								disabled={Boolean(editing.id)}
								onChange={(checked) => setEditing({ ...editing, protected: checked })}
							/>
							<span className="ai-muted">
								{editing.id
									? "受保护标记创建后不可更改（避免把不变式降级成普通配置）。"
									: "受保护 = 后端拒绝修改与删除。"}
							</span>
						</div>
					</div>
				) : null}
			</Modal>
		</>
	);
}
