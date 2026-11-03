/**
 * 赛事管理（控制台）。
 *
 * 破坏性操作的语义在这里尤其重要：删除赛事会连带它的参与记录、题目绑定与实例，
 * 因此删除走**输入确认词**；隐藏（`hidden`）则是可逆的日常操作，不需要确认词。
 * 把"可逆"与"不可逆"分开，用户才不会对每一次点击都麻木。
 */
import { useMemo, useState } from "react";
import { useNavigate } from "react-router";
import {
	Button,
	DatePicker,
	Input,
	Modal,
	Notification,
	Select,
	Switch,
	Tag,
	type TableColumn,
} from "animal-island-ui";
import { CalendarIcon, EditIcon, PlusIcon, TrashIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { Can, useCan } from "~/app/permissions";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { ResourcePage } from "~/components/resource";
import { FAMILY_LABEL, PURPOSE_LABEL, MODE_LABEL, formatTime } from "~/lib/event";
import {
	EventFamily,
	EventPurpose,
	ParticipantMode,
	type Events,
} from "@floatctf/sdk/entity";

type Draft = Partial<Events>;

const emptyDraft = (): Draft => ({
	title: "",
	description: "",
	rules: "",
	hidden: true,
	allow_join: true,
	// 用枚举值而不是字符串字面量：这样后端改枚举名时是**编译错误**，
	// 而不是运行时才发现的静默不一致。
	family: EventFamily.Jeopardy,
	purpose: EventPurpose.Competition,
	participant_mode: ParticipantMode.Individual,
	start_time: new Date().toISOString(),
});

export function ConsoleEvents() {
	const runtime = useRuntime();
	const navigate = useNavigate();
	const confirm = useConfirm();
	const [editing, setEditing] = useState<Draft | null>(null);

	const query = useUniQuery(
		["console", "events"],
		() => runtime.client.admin.events.fetch(),
		"加载赛事",
	);

	const save = useUniMutation(
		(vars: Draft) =>
			vars.id
				? runtime.client.admin.events.patch(vars)
				: runtime.client.admin.events.create(vars),
		"保存赛事",
	);
	const remove = useUniMutation(
		(vars: string[]) => runtime.client.admin.events.remove(vars),
		"删除赛事",
	);

	// 「操作」列整列随权限显隐：只读用户看不到写入口，

	// 也不该看到一个永远为空的操作列（见下面的 columns 过滤）。

	const can = useCan();

	const columns: TableColumn<Events>[] = useMemo(
		() => [
			{ title: "标题", dataIndex: "title", width: 260 },
			{
				title: "赛制",
				dataIndex: "family",
				width: 120,
				render: (v) => (
					<Tag size="small" variant="outlined">
						{FAMILY_LABEL[v as keyof typeof FAMILY_LABEL] ?? String(v)}
					</Tag>
				),
			},
			{
				title: "用途",
				dataIndex: "purpose",
				width: 110,
				render: (v) => (
					<Tag size="small" variant="soft" color="app-blue">
						{PURPOSE_LABEL[v as keyof typeof PURPOSE_LABEL] ?? String(v)}
					</Tag>
				),
			},
			{
				title: "组队",
				dataIndex: "participant_mode",
				width: 90,
				render: (v) => MODE_LABEL[v as keyof typeof MODE_LABEL] ?? String(v),
			},
			{
				title: "开始",
				dataIndex: "start_time",
				width: 170,
				render: (v) => <span className="ai-muted">{formatTime(String(v))}</span>,
			},
			{
				title: "公开",
				dataIndex: "hidden",
				width: 90,
				render: (v) =>
					v ? (
						<Tag size="small" variant="soft" color="brown">
							未公开
						</Tag>
					) : (
						<Tag size="small" variant="soft" color="app-green">
							公开
						</Tag>
					),
			},
			{
				title: "操作",
				width: 200,
				align: "right",
				render: (_v, row) => (
					<div className="ai-row" style={{ justifyContent: "flex-end", gap: 6 }}>
						{/* 「详情」是只读入口，任何有 events.read 的人都能进；写操作才门控。 */}
						<Button
							size="small"
							onClick={() => void navigate(`/console/events/${row.id}`)}
						>
							详情
						</Button>
						<Can permission="events.manage">
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
									title: `删除赛事「${row.title}」`,
									consequence:
										"这会删除该赛事本身，并使其参与名单、题目绑定、实例与解题记录一并失效。**无法恢复**。如果只是想先撤下，请改用编辑窗口里的「对选手公开」开关。",
									okText: "永久删除",
									danger: true,
									confirmPhrase: row.title,
								});
								if (!ok) return;
								await remove.mutateAsync([row.id]);
								Notification.success({ message: "已删除" });
								void query.refetch();
							}}
						>
							删除
						</Button>
						</Can>
					</div>
				),
			},
		],
		[confirm, navigate, query, remove],
	);

	return (
		<>
			<PageHeader
				title="赛事管理"
				description="创建、编辑与下线赛事。删除不可逆，请优先使用「公开」开关。"
				extra={
					<Can permission="events.manage">
						<Button
							type="primary"
							icon={<PlusIcon size={16} />}
							onClick={() => setEditing(emptyDraft())}
						>
							新建赛事
						</Button>
					</Can>
				}
			/>

			<ResourcePage<Events>
				columns={can("events.manage") ? columns : columns.filter((c) => c.title !== "操作")}
				rows={query.data}
				isLoading={query.isLoading}
				error={query.error}
				refetch={() => void query.refetch()}
				rowKey="id"
				emptyTitle="还没有赛事"
				emptyDescription="创建第一场赛事，然后在详情里绑定题目。"
				emptyAction={
					<Can permission="events.manage">
						<Button
							type="primary"
							icon={<PlusIcon size={16} />}
							onClick={() => setEditing(emptyDraft())}
						>
							新建赛事
						</Button>
					</Can>
				}
				disablePagination
			/>

			<Modal
				open={editing !== null}
				title={editing?.id ? "编辑赛事" : "新建赛事"}
				typewriter={false}
				width={620}
				onClose={() => setEditing(null)}
				footer={
					<>
						<Button onClick={() => setEditing(null)}>取消</Button>
						<Button
							type="primary"
							disabled={!editing?.title?.trim()}
							loading={save.isPending}
							onClick={async () => {
								if (!editing) return;
								await save.mutateAsync({
									...editing,
									title: editing.title?.trim(),
									// 新建时 family/purpose 无 id 也可提交；后端按字段处理
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
							<span>标题</span>
							<Input
								value={editing.title ?? ""}
								onChange={(e) => setEditing({ ...editing, title: e.target.value })}
							/>
						</label>

						<label className="ai-col" style={{ gap: 6 }}>
							<span>简介</span>
							<Input
								value={editing.description ?? ""}
								onChange={(e) => setEditing({ ...editing, description: e.target.value })}
							/>
						</label>

						<label className="ai-col" style={{ gap: 6 }}>
							<span>规则</span>
							<Input
								value={editing.rules ?? ""}
								onChange={(e) => setEditing({ ...editing, rules: e.target.value })}
							/>
						</label>

						<div className="ai-row" style={{ gap: 12, alignItems: "flex-end" }}>
							<div style={{ minWidth: 170 }}>
								<span>赛制</span>
								<Select
									value={editing.family ?? EventFamily.Jeopardy}
									onChange={(v) =>
										setEditing({ ...editing, family: v as Events["family"] })
									}
									// 赛制在创建后改变会破坏已有数据，因此编辑态禁用。
									disabled={Boolean(editing.id)}
									options={[
										{ key: EventFamily.Jeopardy, label: "Jeopardy（解题）" },
										{ key: EventFamily.Awd, label: "AWD（攻防）" },
										{ key: EventFamily.Awdp, label: "AWDP（漏洞修复）" },
									]}
								/>
							</div>
							<div style={{ minWidth: 150 }}>
								<span>用途</span>
								<Select
									value={editing.purpose ?? EventPurpose.Competition}
									onChange={(v) =>
										setEditing({ ...editing, purpose: v as Events["purpose"] })
									}
									options={[
										{ key: EventPurpose.Competition, label: "正式比赛" },
										{ key: EventPurpose.Practice, label: "练习" },
									]}
								/>
							</div>
							<div style={{ minWidth: 130 }}>
								<span>组队</span>
								<Select
									value={editing.participant_mode ?? ParticipantMode.Individual}
									onChange={(v) =>
										setEditing({
											...editing,
											participant_mode: v as Events["participant_mode"],
										})
									}
									options={[
										{ key: ParticipantMode.Individual, label: "个人" },
										{ key: ParticipantMode.Team, label: "团队" },
									]}
								/>
							</div>
						</div>

						<label className="ai-col" style={{ gap: 6 }}>
							<span>开始时间</span>
							<DatePicker
								value={editing.start_time ? editing.start_time.slice(0, 10) : null}
								onChange={(v) =>
									setEditing({
										...editing,
										// DatePicker 只给日期；保留原有时分秒，避免把时间悄悄归零。
										start_time: `${String(v)}T${(editing.start_time ?? "00:00:00").slice(11, 19) || "00:00:00"}`,
									})
								}
								allowClear
							/>
						</label>

						<div className="ai-row" style={{ gap: 24 }}>
							<label className="ai-row" style={{ gap: 8 }}>
								<Switch
									checked={!editing.hidden}
									onChange={(checked) => setEditing({ ...editing, hidden: !checked })}
								/>
								<span>对选手公开</span>
							</label>
							<label className="ai-row" style={{ gap: 8 }}>
								<Switch
									checked={Boolean(editing.allow_join)}
									onChange={(checked) => setEditing({ ...editing, allow_join: checked })}
								/>
								<span>允许自行加入</span>
							</label>
						</div>

						<div className="ai-row" style={{ gap: 8 }}>
							<CalendarIcon size={16} />
							<span className="ai-muted">
								{editing.id
									? "修改赛制会破坏已有题目绑定与计分，因此编辑时不可改。"
									: "赛制决定后续可用的功能，创建后不可更改。"}
							</span>
						</div>
					</div>
				) : null}
			</Modal>
		</>
	);
}
