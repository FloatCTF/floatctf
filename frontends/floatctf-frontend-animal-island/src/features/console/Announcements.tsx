/**
 * 公告管理。
 *
 * 公告的 `publisher` 是**快照**：v2 起 `publisher_id` 可空且账号删除时置 NULL，
 * 但 `publisher` 文本保留 —— 因此这里把两者都显示出来，让管理员看到
 * "发布者账号已注销"这种情况，而不是让它静默变成一个空名字。
 */
import { useState } from "react";
import { Button, Input, Modal, Notification, Tag, type TableColumn } from "animal-island-ui";
import { EditIcon, PlusIcon, TrashIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { Can, useCan } from "~/app/permissions";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { ResourcePage } from "~/components/resource";
import { formatTime } from "~/lib/event";
import type { Announcements } from "@floatctf/sdk/entity";

export function ConsoleAnnouncements() {
	const runtime = useRuntime();
	const confirm = useConfirm();
	const [editing, setEditing] = useState<Partial<Announcements> | null>(null);

	const query = useUniQuery(
		["console", "announcements"],
		() => runtime.client.admin.announcements.fetch(),
		"加载公告",
	);
	const save = useUniMutation(
		(vars: Partial<Announcements>) =>
			vars.id
				? runtime.client.admin.announcements.patch(vars)
				: runtime.client.admin.announcements.create(vars),
		"保存公告",
	);
	const remove = useUniMutation(
		(vars: string[]) => runtime.client.admin.announcements.remove(vars),
		"删除公告",
	);

	// 「操作」列整列随权限显隐：只读用户看不到写入口，

	// 也不该看到一个永远为空的操作列（见下面的 columns 过滤）。

	const can = useCan();

	const columns: TableColumn<Announcements>[] = [
		{ title: "标题", dataIndex: "title", width: 260 },
		{
			title: "发布者",
			dataIndex: "publisher",
			width: 200,
			render: (v, row) => (
				<span className="ai-row" style={{ gap: 6 }}>
					{String(v)}
					{/* publisher_id 为空 = 该账号已被删除；这是 v2 的预期行为，不是数据损坏。 */}
					{row.publisher_id === null || row.publisher_id === undefined ? (
						<Tag size="small" variant="soft" color="brown">
							账号已注销
						</Tag>
					) : null}
				</span>
			),
		},
		{
			title: "更新时间",
			dataIndex: "updated_at",
			width: 170,
			render: (v) => <span className="ai-muted">{formatTime(String(v))}</span>,
		},
		{
			title: "操作",
			width: 170,
			align: "right",
			render: (_v, row) => (
				<Can permission="announcements.manage">
				<div className="ai-row" style={{ justifyContent: "flex-end", gap: 6 }}>
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
								title: `删除公告「${row.title}」`,
								consequence: "选手侧将立刻看不到这条公告。删除后无法恢复。",
								okText: "删除",
								danger: true,
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
				title="公告"
				description="面向全平台的公告。发布者是快照：账号注销后文字仍保留。"
				extra={
					<Can permission="announcements.manage">
						<Button
							type="primary"
							icon={<PlusIcon size={16} />}
							onClick={() => setEditing({ title: "", content: "" })}
						>
							新建公告
						</Button>
					</Can>
				}
			/>

			<ResourcePage<Announcements>
				columns={can("announcements.manage") ? columns : columns.filter((c) => c.title !== "操作")}
				rows={query.data}
				isLoading={query.isLoading}
				error={query.error}
				refetch={() => void query.refetch()}
				rowKey="id"
				emptyTitle="还没有公告"
				emptyDescription="给选手发一条公告吧。"
				disablePagination
			/>

			<Modal
				open={editing !== null}
				title={editing?.id ? "编辑公告" : "新建公告"}
				typewriter={false}
				width={620}
				onClose={() => setEditing(null)}
				footer={
					<>
						<Button onClick={() => setEditing(null)}>取消</Button>
						<Button
							type="primary"
							disabled={!editing?.title?.trim() || !editing?.content?.trim()}
							loading={save.isPending}
							onClick={async () => {
								if (!editing) return;
								await save.mutateAsync({
									...editing,
									title: editing.title?.trim(),
									content: editing.content?.trim(),
								});
								Notification.success({ message: editing.id ? "已更新" : "已发布" });
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
						<span>标题</span>
						<Input
							value={editing?.title ?? ""}
							onChange={(e) => setEditing({ ...editing, title: e.target.value })}
						/>
					</label>
					<label className="ai-col" style={{ gap: 6 }}>
						<span>正文</span>
						<Input
							value={editing?.content ?? ""}
							placeholder="支持纯文本，换行会保留"
							onChange={(e) => setEditing({ ...editing, content: e.target.value })}
						/>
					</label>
				</div>
			</Modal>
		</>
	);
}
