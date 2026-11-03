/**
 * 计划任务。
 *
 * `protected` 任务由平台引导创建（例如判题队列清理），后端拒绝删除；
 * 界面禁用删除入口并说明原因。
 *
 * 「立即执行」是**会产生真实副作用**的操作（发邮件、清库、回收实例），
 * 因此不放在列表里随手点，而是走确认弹窗；任务名在确认文案里写清楚。
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
	Tooltip,
	type TableColumn,
} from "animal-island-ui";
import { EditIcon, LockIcon, PlayIcon, PlusIcon, RefreshIcon, TrashIcon } from "naive-icons";

import { Can, useCan } from "~/app/permissions";
import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { ResourcePage } from "~/components/resource";
import { formatTime } from "~/lib/event";
// `trigger_type` / `status` 在生成实体里是**普通字符串列**（没有对应的 PG enum），
// 因此这里比较字符串而不是引用枚举 —— 硬套一个不存在的枚举会假装有约束。
import type { ScheduledTasks } from "@floatctf/sdk/entity";

export function ConsoleTasks() {
	const runtime = useRuntime();
	const confirm = useConfirm();
	// 开关无法"隐藏"（会破坏列布局），因此这里用 can() 决定是否禁用；
	// 其余动作按钮用 <Can> 直接不渲染。
	const can = useCan();
	const [editing, setEditing] = useState<Partial<ScheduledTasks> | null>(null);

	const query = useUniQuery(
		["console", "tasks"],
		() => runtime.client.admin.scheduled_tasks.fetch(),
		"加载计划任务",
	);
	const save = useUniMutation(
		(vars: Partial<ScheduledTasks>) =>
			vars.id
				? runtime.client.admin.scheduled_tasks.patch(vars)
				: runtime.client.admin.scheduled_tasks.create(vars),
		"保存任务",
	);
	const remove = useUniMutation(
		(vars: string[]) => runtime.client.admin.scheduled_tasks.remove(vars),
		"删除任务",
	);
	const run = useUniMutation(
		(vars: string) => runtime.client.admin.scheduled_tasks.run(vars),
		"执行任务",
	);

	const columns: TableColumn<ScheduledTasks>[] = [
		{ title: "任务", dataIndex: "task_name", width: 220 },
		{
			title: "标识",
			dataIndex: "task_key",
			width: 180,
			render: (v) => (
				<Tag size="small" variant="outlined">
					{String(v)}
				</Tag>
			),
		},
		{
			title: "触发",
			dataIndex: "trigger_type",
			width: 120,
			render: (v) => (
				<Tag size="small" variant="outlined">
					{String(v)}
				</Tag>
			),
		},
		{
			title: "状态",
			dataIndex: "status",
			width: 110,
			render: (v) => {
				const s = String(v);
				const color =
					s === "success" ? "app-green" : s === "failed" ? "app-red" : s === "running" ? "app-blue" : "brown";
				return (
					<Tag size="small" variant="soft" color={color}>
						{s}
					</Tag>
				);
			},
		},
		{
			title: "重试",
			dataIndex: "attempt_count",
			width: 100,
			render: (v, row) => (
				<span className="ai-muted">
					{String(v)}/{row.max_attempts}
				</span>
			),
		},
		{
			title: "启用",
			dataIndex: "enabled",
			width: 90,
			render: (v, row) => (
				<Switch
					checked={Boolean(v)}
					// 无权限时不仅隐藏入口，开关本身也禁用（开关无法"隐藏"而不破坏列布局）。
					disabled={Boolean(row.protected) || !can("scheduler.manage")}
					onChange={async (checked) => {
						await runtime.client.admin.scheduled_tasks.patch({
							id: row.id,
							enabled: checked,
						});
						void query.refetch();
					}}
				/>
			),
		},
		{
			title: "最近更新",
			dataIndex: "updated_at",
			width: 160,
			render: (v) => <span className="ai-muted">{formatTime(String(v))}</span>,
		},
		{
			title: "操作",
			width: 230,
			align: "right",
			render: (_v, row) => (
				<Can permission="scheduler.manage">
				<div className="ai-row" style={{ justifyContent: "flex-end", gap: 6 }}>
					<Button
						size="small"
						icon={<PlayIcon size={14} />}
						loading={run.isPending}
						onClick={async () => {
							const ok = await confirm({
								title: `立即执行「${row.task_name}」`,
								consequence:
									"这会**立刻真实执行**该任务（可能发送通知、回收实例或清理数据），而不是等下一次调度。执行期间不要重复点击。",
								okText: "立即执行",
								danger: true,
							});
							if (!ok) return;
							await run.mutateAsync(row.id);
							Notification.success({ message: "已触发执行" });
							void query.refetch();
						}}
					>
						执行
					</Button>
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
						disabled={Boolean(row.protected)}
						onClick={async () => {
							const ok = await confirm({
								title: `删除计划任务「${row.task_name}」`,
								consequence:
									"任务会被移除，**它负责的周期性工作将不再发生**（例如实例回收、日志清理）。这类问题通常在几天后才暴露。",
								okText: "删除任务",
								danger: true,
								confirmPhrase: row.task_key,
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
				title="计划任务"
				description="平台周期性工作。受保护的任务由引导创建，不可删除。"
				extra={
					<>
						<Button
							size="small"
							icon={<RefreshIcon size={16} />}
							onClick={() => void query.refetch()}
						>
							刷新
						</Button>
						<Can permission="scheduler.manage">
						<Button
							type="primary"
							icon={<PlusIcon size={16} />}
							onClick={() =>
								setEditing({
									task_name: "",
									task_key: "",
									trigger_type: "interval",
									enabled: true,
								})
							}
						>
							新建任务
						</Button>
						</Can>
					</>
				}
			/>

			<ResourcePage<ScheduledTasks>
				columns={can("scheduler.manage") ? columns : columns.filter((c) => c.title !== "操作")}
				rows={query.data}
				isLoading={query.isLoading}
				error={query.error}
				refetch={() => void query.refetch()}
				rowKey="id"
				emptyTitle="还没有计划任务"
				disablePagination
			/>

			<Modal
				open={editing !== null}
				title={editing?.id ? "编辑任务" : "新建任务"}
				typewriter={false}
				onClose={() => setEditing(null)}
				footer={
					<>
						<Button onClick={() => setEditing(null)}>取消</Button>
						<Button
							type="primary"
							disabled={!editing?.task_name?.trim() || !editing?.task_key?.trim()}
							loading={save.isPending}
							onClick={async () => {
								if (!editing) return;
								await save.mutateAsync({
									...editing,
									task_name: editing.task_name?.trim(),
									task_key: editing.task_key?.trim(),
								});
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
							<span>任务名</span>
							<Input
								value={editing.task_name ?? ""}
								onChange={(e) => setEditing({ ...editing, task_name: e.target.value })}
							/>
						</label>
						<label className="ai-col" style={{ gap: 6 }}>
							<span>标识（后端用它在代码里认领实现）</span>
							<Input
								value={editing.task_key ?? ""}
								disabled={Boolean(editing.id)}
								placeholder="例如 clean_expired_instances"
								onChange={(e) => setEditing({ ...editing, task_key: e.target.value })}
							/>
							<Tooltip title="标识必须与后端已实现的调度器键一致，否则任务不会被真实处理">
								<span className="ai-muted" style={{ fontSize: 12 }}>
									标识要和后端实现对应，界面无法替你校验。
								</span>
							</Tooltip>
						</label>
						<div className="ai-col" style={{ gap: 6 }}>
							<span>触发方式</span>
							<Select
								value={editing.trigger_type ?? "interval"}
								onChange={(v) =>
									setEditing({ ...editing, trigger_type: v as ScheduledTasks["trigger_type"] })
								}
								options={[
									{ key: "interval", label: "固定间隔" },
									{ key: "cron", label: "Cron 表达式" },
									{ key: "manual", label: "仅手动" },
								]}
							/>
						</div>
						<div className="ai-row" style={{ gap: 8 }}>
							<Switch
								checked={Boolean(editing.enabled)}
								onChange={(checked) => setEditing({ ...editing, enabled: checked })}
							/>
							<span>启用</span>
						</div>
						{editing.protected ? (
							<div className="ai-row" style={{ gap: 8 }}>
								<LockIcon size={16} />
								<span className="ai-muted">
									这是受保护任务：可以调整启用状态，但不可删除。
								</span>
							</div>
						) : null}
					</div>
				) : null}
			</Modal>
		</>
	);
}
