/**
 * 赛事详情（控制台）。
 *
 * 这一页是"赛事运营视图"：规模、Top10、题目解出分布、最近流水，
 * 以及导出（报告 / Writeup）。刻意**不做**成数据大屏的复刻 ——
 * 大屏是给现场投屏看的（`getReport` 另开窗口），这里是给人操作看的。
 */
import { useState } from "react";
import { useNavigate, useParams } from "react-router";
import {
	Avatar,
	Button,
	Card,
	Divider,
	Input,
	Loading,
	Modal,
	Notification,
	Select,
	Tabs,
	Tag,
	type TableColumn,
} from "animal-island-ui";
import {
	ArrowLeftIcon,
	DownloadIcon,
	FlagIcon,
	MagnetIcon,
	PlusIcon,
	RefreshIcon,
	ShieldIcon,
	TrashIcon,
	TrophyIcon,
	UsersIcon,
} from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { Can } from "~/app/permissions";
import { PageHeader } from "~/app/shell";
import { ResourcePage, StatCard } from "~/components/resource";
import { useConfirm } from "~/components/confirm";
import { EmptyState, ErrorState, SkeletonCards } from "~/components/states";
import { FAMILY_LABEL, MODE_LABEL, PURPOSE_LABEL, formatTime } from "~/lib/event";
import type { AdminInstanceRow, EventUserResult } from "@floatctf/sdk";
// `EventAnnouncements` 是生成实体，只从 `/entity` 子路径导出。
import type { EventAnnouncements } from "@floatctf/sdk/entity";

/**
 * 赛事参与用户。
 *
 * ## 关于 `password`（曾经的坑，已修）
 *
 * 这个端点**曾经**把参与者的 argon2id 哈希回传给客户端：
 * 后端返回的 `EventUserResult.user` 是生成的实体 `users::Model`（DB 列镜像，含 `password`），
 * 且 `Users` 这个类型还被写进了 SDK 与前端的类型定义里，等于邀请调用方去读它。
 *
 * 根因是「不变式的作用域是全平台，守卫的作用域却只有一个模块」。现已修复：
 * - 后端 `EventUserResult.user` 改成 `identity::user::dto::UsersDto`；
 * - SDK `EventUserResult.user` 改成 `UserDto`；
 * - 新增**全仓**源码守卫 `no_serialized_response_struct_holds_a_sensitive_entity_model`
 *   与运行时断言 `event_user_result_serializes_without_password`。
 *
 * 本组件只取 `username` / `nickname` / `email` / `avatar` / `event_user.*`，
 * 并且**永不**渲染 `password` —— 即使将来有人把字段加回去，这里也不会显示它。
 */
function EventUsersPanel({ eventId }: { eventId: string }) {
	const runtime = useRuntime();
	const confirm = useConfirm();
	const api = runtime.client.admin.event_users;

	const users = useUniQuery(
		["event-users", eventId],
		() => api.fetch(eventId)(),
		"加载参与用户",
	);
	const allUsers = useUniQuery(
		["console", "users"],
		() => runtime.client.admin.users.fetch(),
		"加载账号列表",
	);
	const add = useUniMutation(
		(vars: { userId: string }) => api.add({ event_id: eventId, user_id: vars.userId }),
		"添加参与用户",
	);
	const remove = useUniMutation(
		(vars: string[]) => api.delete(eventId)(vars),
		"移除参与用户",
	);
	const ban = useUniMutation(
		(vars: { userId: string }) => api.banned({ event_id: eventId, user_id: vars.userId }),
		"封禁",
	);
	const unban = useUniMutation(
		(vars: { userId: string }) => api.unbanned({ event_id: eventId, user_id: vars.userId }),
		"解封",
	);

	const joined = new Set((users.data ?? []).map((u) => u.user.id));
	const candidates = (allUsers.data ?? []).filter((u) => !joined.has(u.id));

	const columns: TableColumn<EventUserResult>[] = [
		{
			title: "用户",
			width: 260,
			render: (_v, row) => (
				<div className="ai-row" style={{ gap: 8 }}>
					<Avatar size="small" src={row.user.avatar ?? undefined}>
						{row.user.nickname?.[0] ?? "?"}
					</Avatar>
					<div className="ai-col" style={{ gap: 0, alignItems: "flex-start" }}>
						<span className="ai-strong">{row.user.nickname}</span>
						<span className="ai-muted" style={{ fontSize: 12 }}>
							@{row.user.username}
						</span>
					</div>
				</div>
			),
		},
		{ title: "邮箱", dataIndex: "user", width: 220, render: (_v, row) => row.user.email },
		{
			title: "赛事分",
			width: 100,
			align: "right",
			render: (_v, row) => <span className="ai-strong">{row.event_user.points}</span>,
		},
		{
			title: "状态",
			width: 110,
			render: (_v, row) =>
				row.event_user.banned ? (
					<Tag size="small" variant="soft" color="app-red">
						已封禁
					</Tag>
				) : (
					<Tag size="small" variant="soft" color="app-green">
						正常
					</Tag>
				),
		},
		{
			title: "加入时间",
			width: 160,
			render: (_v, row) => (
				<span className="ai-muted">{formatTime(row.event_user.joined_at)}</span>
			),
		},
		{
			title: "操作",
			width: 190,
			align: "right",
			render: (_v, row) => (
				<Can permission="events.participants.manage">
				<div className="ai-row" style={{ justifyContent: "flex-end", gap: 6 }}>
					{row.event_user.banned ? (
						<Button
							size="small"
							icon={<ShieldIcon size={14} />}
							loading={unban.isPending}
							onClick={async () => {
								await unban.mutateAsync({ userId: row.user.id });
								void users.refetch();
							}}
						>
							解封
						</Button>
					) : (
						<Button
							size="small"
							danger
							icon={<ShieldIcon size={14} />}
							loading={ban.isPending}
							onClick={async () => {
								const ok = await confirm({
									title: `封禁「${row.user.nickname}」在这场比赛中的参赛资格`,
									consequence:
										"该用户在本场赛事内**不能提交 flag**（这是**赛事级**封禁，不影响其在其它赛事的参赛资格，也不影响账号本身）。",
									okText: "封禁",
									danger: true,
								});
								if (!ok) return;
								await ban.mutateAsync({ userId: row.user.id });
								void users.refetch();
							}}
						>
							封禁
						</Button>
					)}
					<Button
						size="small"
						type="text"
						danger
						icon={<TrashIcon size={14} />}
						onClick={async () => {
							const ok = await confirm({
								title: `把「${row.user.nickname}」移出本场赛事`,
								consequence:
									"该用户会从本场参赛名单中移除，**不再计入排行**。其历史解题记录仍保留在数据库，但需要重新加入才能继续提交。",
								okText: "移出",
								danger: true,
							});
							if (!ok) return;
							await remove.mutateAsync([row.event_user.user_id]);
							void users.refetch();
						}}
					>
						移出
					</Button>
				</div>
				</Can>
			),
		},
	];

	return (
		<div className="ai-col" style={{ gap: 12 }}>
			<Card>
				<div className="ai-row-between">
					<span className="ai-muted">
						参赛名单决定谁能在本场赛事里提交。手动加入用于补录或定向邀请。
					</span>
					<Can permission="events.participants.manage">
					<div style={{ minWidth: 280 }}>
						<Select
							value=""
							placeholder="选择一个账号加入本场"
							showSearch
							options={candidates.map((u) => ({
								key: u.id,
								label: `${u.nickname} (@${u.username})`,
							}))}
							onChange={async (v) => {
								if (typeof v !== "string" || !v) return;
								await add.mutateAsync({ userId: v });
								Notification.success({ message: "已加入" });
								void users.refetch();
							}}
						/>
					</div>
					</Can>
				</div>
			</Card>

			<ResourcePage<EventUserResult>
				columns={columns}
				rows={users.data}
				isLoading={users.isLoading || allUsers.isLoading}
				error={users.error}
				refetch={() => void users.refetch()}
				rowKey="id"
				emptyTitle="还没有参赛用户"
				emptyDescription="选手自行加入，或在这里手动添加。"
				disablePagination
			/>
		</div>
	);
}

/** 赛事公告（与平台级公告是两回事）。 */
function EventAnnouncementsPanel({ eventId }: { eventId: string }) {
	const runtime = useRuntime();
	const confirm = useConfirm();
	const api = runtime.client.admin.event_announcements;

	const [editing, setEditing] = useState<Partial<EventAnnouncements> | null>(null);
	const list = useUniQuery(
		["event-announcements", eventId],
		() => api.fetch(eventId)(),
		"加载赛事公告",
	);
	const create = useUniMutation(
		(vars: Partial<EventAnnouncements>) => api.create(eventId)(vars),
		"发布公告",
	);
	const patch = useUniMutation(
		(vars: Partial<EventAnnouncements>) => api.patch(eventId)(vars),
		"更新公告",
	);
	const remove = useUniMutation(
		(vars: string[]) => api.remove(eventId)(vars),
		"删除公告",
	);

	const columns: TableColumn<EventAnnouncements>[] = [
		{ title: "标题", dataIndex: "title", width: 260 },
		{
			title: "正文",
			dataIndex: "content",
			render: (v) => (
				<span className="ai-muted" style={{ fontSize: 13 }}>
					{String(v).length > 90 ? `${String(v).slice(0, 90)}…` : String(v)}
				</span>
			),
		},
		{
			title: "发布时间",
			dataIndex: "created_at",
			width: 170,
			render: (v) => <span className="ai-muted">{formatTime(String(v))}</span>,
		},
		{
			title: "操作",
			width: 160,
			align: "right",
			render: (_v, row) => (
				<div className="ai-row" style={{ justifyContent: "flex-end", gap: 6 }}>
					<Can permission="events.manage">
					<Button size="small" type="text" onClick={() => setEditing({ ...row })}>
						编辑
					</Button>
					<Button
						size="small"
						type="text"
						danger
						icon={<TrashIcon size={14} />}
						onClick={async () => {
							const ok = await confirm({
								title: `删除赛事公告「${row.title}」`,
								consequence: "本场赛事的选手将立刻看不到这条公告。删除后无法恢复。",
								okText: "删除",
								danger: true,
							});
							if (!ok) return;
							await remove.mutateAsync([row.id]);
							void list.refetch();
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
		<div className="ai-col" style={{ gap: 12 }}>
			<Card>
				<div className="ai-row-between">
					<span className="ai-muted">
						这是**赛事内**公告，只对本场选手可见；平台级公告在「公告」页。
					</span>
					<Can permission="events.manage">
						<Button
							type="primary"
							icon={<PlusIcon size={16} />}
							onClick={() => setEditing({ title: "", content: "" })}
						>
							发布公告
						</Button>
					</Can>
				</div>
			</Card>

			<ResourcePage<EventAnnouncements>
				columns={columns}
				rows={list.data}
				isLoading={list.isLoading}
				error={list.error}
				refetch={() => void list.refetch()}
				rowKey="id"
				emptyTitle="还没有赛事公告"
				disablePagination
			/>

			<Modal
				open={editing !== null}
				title={editing?.id ? "编辑赛事公告" : "发布赛事公告"}
				typewriter={false}
				onClose={() => setEditing(null)}
				footer={
					<>
						<Button onClick={() => setEditing(null)}>取消</Button>
						<Button
							type="primary"
							disabled={!editing?.title?.trim() || !editing?.content?.trim()}
							loading={create.isPending || patch.isPending}
							onClick={async () => {
								if (!editing) return;
								const payload = {
									...editing,
									title: editing.title?.trim(),
									content: editing.content?.trim(),
								};
								if (editing.id) await patch.mutateAsync(payload);
								else await create.mutateAsync(payload);
								Notification.success({ message: editing.id ? "已更新" : "已发布" });
								setEditing(null);
								void list.refetch();
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
							onChange={(e) => setEditing({ ...editing, content: e.target.value })}
						/>
					</label>
				</div>
			</Modal>
		</div>
	);
}

/** 赛事统一实例列表：challenge + gamebox 归一化。 */
function EventInstancesPanel({ eventId }: { eventId: string }) {
	const runtime = useRuntime();
	const q = useUniQuery(
		["event-instances", eventId],
		() => runtime.client.admin.instances.listForEvent(eventId),
		"加载实例",
	);

	const columns: TableColumn<AdminInstanceRow>[] = [
		{
			title: "类型",
			dataIndex: "instance_type",
			width: 100,
			render: (v) => (
				<Tag size="small" variant="outlined">
					{v === "gamebox" ? "靶机" : "题目"}
				</Tag>
			),
		},
		{ title: "内容", dataIndex: "content_title", width: 200 },
		{
			title: "归属",
			width: 200,
			render: (_v, row) => (
				<span>
					{row.team_name ? `${row.team_name} ｜ ` : ""}
					{row.user_name ?? "—"}
				</span>
			),
		},
		{
			title: "状态",
			dataIndex: "status",
			width: 120,
			render: (v) => (
				<Tag size="small" variant="soft">
					{String(v)}
				</Tag>
			),
		},
		{
			title: "标识",
			dataIndex: "identifier",
			width: 180,
			render: (v) => (
				<span className="ai-mono" style={{ fontSize: 12 }}>
					{String(v).slice(0, 16)}
				</span>
			),
		},
		{
			title: "回收时间",
			dataIndex: "destroy_at",
			width: 170,
			render: (v) => (
				<span className="ai-muted">{v ? formatTime(String(v)) : "—"}</span>
			),
		},
	];

	return (
		<ResourcePage<AdminInstanceRow>
			columns={columns}
			rows={q.data}
			isLoading={q.isLoading}
			error={q.error}
			refetch={() => void q.refetch()}
			rowKey="id"
			emptyTitle="本场还没有实例"
			emptyDescription="选手启动题目或靶机后，这里会出现归一化的实例列表。"
			disablePagination
		/>
	);
}

export function ConsoleEventDetail() {
	const { eventId = "" } = useParams();
	const runtime = useRuntime();
	const navigate = useNavigate();
	const [report, setReport] = useState<string | null>(null);

	const event = useUniQuery(
		["console", "event", eventId],
		() => runtime.client.admin.events.get(eventId),
		"加载赛事",
	);
	const data = useUniQuery(
		["console", "event", eventId, "data"],
		() => runtime.client.admin.events.getData(eventId),
		"加载赛事数据",
	);

	const reportFn = useUniMutation(
		() => runtime.client.admin.events.getReport(eventId),
		"生成报告",
	);
	const exportWp = useUniMutation(
		() => runtime.client.admin.events.exportWriteUps(eventId),
		"导出 Writeup",
	);

	if (event.isLoading) return <Loading active tip="加载赛事…" />;
	if (event.error) return <ErrorState error={event.error} onRetry={() => void event.refetch()} />;
	if (!event.data) {
		return (
			<Card>
				<EmptyState
					title="赛事不存在"
					description="它可能已被删除。"
					action={<Button onClick={() => void navigate("/console/events")}>回赛事管理</Button>}
				/>
			</Card>
		);
	}

	const e = event.data;

	return (
		<>
			<PageHeader
				title={e.title}
				description={e.description ?? "（没有写简介）"}
				extra={
					<>
						<Button
							size="small"
							icon={<ArrowLeftIcon size={16} />}
							onClick={() => void navigate("/console/events")}
						>
							回列表
						</Button>
						<Button
							size="small"
							icon={<RefreshIcon size={16} />}
							onClick={() => {
								void event.refetch();
								void data.refetch();
							}}
						>
							刷新
						</Button>
						<Button
							size="small"
							icon={<DownloadIcon size={16} />}
							loading={reportFn.isPending}
							onClick={async () => {
								const text = await reportFn.mutateAsync(undefined);
								setReport(text ?? "（报告为空）");
							}}
						>
							赛事报告
						</Button>
						<Button
							size="small"
							icon={<DownloadIcon size={16} />}
							loading={exportWp.isPending}
							onClick={async () => {
								const text = await exportWp.mutateAsync(undefined);
								setReport(text ?? "（导出为空）");
								Notification.success({ message: "Writeup 已导出" });
							}}
						>
							导出 Writeup
						</Button>
					</>
				}
			/>

			<Card>
				<div className="ai-row" style={{ gap: 14, flexWrap: "wrap" }}>
					<Tag size="small" variant="solid" color="app-teal">
						{FAMILY_LABEL[e.family]}
					</Tag>
					<Tag size="small" variant="outlined">
						{PURPOSE_LABEL[e.purpose]}
					</Tag>
					<Tag size="small" variant="outlined">
						{MODE_LABEL[e.participant_mode]}
					</Tag>
					{e.hidden ? (
						<Tag size="small" variant="soft" color="brown">
							未公开
						</Tag>
					) : null}
					<Tag size="small" variant={e.allow_join ? "soft" : "outlined"} color="app-green">
						{e.allow_join ? "允许自行加入" : "仅邀请"}
					</Tag>
					<span className="ai-muted">开始：{formatTime(e.start_time)}</span>
					<span className="ai-muted">结束：{formatTime(e.end_time)}</span>
				</div>
			</Card>

			{e.family === "awdp" ? (
				<Card>
					<div className="ai-row-between">
						<div className="ai-col" style={{ gap: 2, alignItems: "flex-start" }}>
							<span className="ai-strong">AWDP 控制台</span>
							<span className="ai-muted" style={{ fontSize: 12 }}>
								阶段推进（开始 / 转入 Fix / 结束）、参数配置、题目挂载、实例与积分。
							</span>
						</div>
						<Button
							type="primary"
							icon={<ShieldIcon size={16} />}
							onClick={() => void navigate(`/console/awdp/${eventId}`)}
						>
							进入 AWDP 控制台
						</Button>
					</div>
				</Card>
			) : null}

			{e.family === "awd" ? (
				<Card>
					<div className="ai-row-between">
						<div className="ai-col" style={{ gap: 2, alignItems: "flex-start" }}>
							<span className="ai-strong">AWD 控制台</span>
							<span className="ai-muted" style={{ fontSize: 12 }}>
								生命周期（部署/开始/暂停/结束/归档）、预检与凭据轮换、封禁与调分、
								靶机挂载、网络分配。
							</span>
						</div>
						<Button
							type="primary"
							icon={<ShieldIcon size={16} />}
							onClick={() => void navigate(`/console/awd/${eventId}`)}
						>
							进入 AWD 控制台
						</Button>
					</div>
				</Card>
			) : null}

			{data.isLoading ? (
				<SkeletonCards count={4} />
			) : data.error ? (
				<ErrorState error={data.error} onRetry={() => void data.refetch()} />
			) : data.data ? (
				<>
					<div className="ai-grid">
						<StatCard
							label="参与人数"
							value={data.data.user_count}
							icon={<UsersIcon size={18} />}
						/>
						<StatCard
							label="战队数"
							value={data.data.team_count}
							icon={<UsersIcon size={18} />}
						/>
						<StatCard
							label="题目数"
							value={data.data.event_challenges.length}
							icon={<MagnetIcon size={18} />}
						/>
						<StatCard
							label="最近 15 分钟解出"
							value={data.data.solved_recent_15.length}
							icon={<FlagIcon size={18} />}
						/>
					</div>

					<div className="ai-grid ai-grid-wide">
						<Card>
							<div className="ai-col" style={{ gap: 10 }}>
								<div className="ai-row" style={{ gap: 8 }}>
									<TrophyIcon size={18} />
									<span className="ai-strong">Top 10</span>
								</div>
								<Divider />
								{data.data.scoreboard_top10.length === 0 ? (
									<EmptyState title="还没有队伍得分" />
								) : (
									data.data.scoreboard_top10.map((row) => (
										<div key={row.id} className="ai-row-between">
											<div className="ai-row" style={{ gap: 8 }}>
												<Tag size="small" variant="soft">
													#{row.no}
												</Tag>
												<span>{row.name}</span>
											</div>
											<span className="ai-strong">{row.score}</span>
										</div>
									))
								)}
							</div>
						</Card>

						<Card>
							<div className="ai-col" style={{ gap: 10 }}>
								<span className="ai-strong">题目解出分布</span>
								<Divider />
								{data.data.event_challenges.length === 0 ? (
									<EmptyState title="这场赛事还没有绑定题目" />
								) : (
									data.data.event_challenges.map((c) => (
										<div key={c.name} className="ai-row-between">
											<div className="ai-row" style={{ gap: 8 }}>
												<span>{c.name}</span>
												<Tag size="small" variant="outlined">
													{c.category}
												</Tag>
											</div>
											<div className="ai-row" style={{ gap: 8 }}>
												<span className="ai-muted" style={{ fontSize: 12 }}>
													{c.points} 分
												</span>
												<Tag size="small" variant="soft" color="app-teal">
													{c.solved_count} 解 / {c.solved_percent.toFixed(0)}%
												</Tag>
											</div>
										</div>
									))
								)}
							</div>
						</Card>
					</div>

					<Card>
						<div className="ai-col" style={{ gap: 10 }}>
							<span className="ai-strong">最近解出流水</span>
							<Divider />
							{data.data.solved_recent_15.length === 0 ? (
								<EmptyState title="最近没有新的解出记录" />
							) : (
								data.data.solved_recent_15.map((s, i) => (
									<div key={`${s.user_nickname}-${s.created_at}-${i}`} className="ai-row-between">
										<div className="ai-row" style={{ gap: 8 }}>
											<span className="ai-strong">{s.user_nickname}</span>
											<span className="ai-muted">解出了</span>
											<span>{s.challenge_name}</span>
											<Tag size="small" variant="outlined">
												{s.challenge_category}
											</Tag>
											{s.bonus_points > 0 ? (
												<Tag size="small" variant="solid" color="app-orange">
													首杀 +{s.bonus_points}
												</Tag>
											) : null}
										</div>
										<span className="ai-muted" style={{ fontSize: 12 }}>
											{formatTime(s.created_at)}
										</span>
									</div>
								))
							)}
						</div>
					</Card>
				</>
			) : null}

			<Card>
				<Tabs
					aria-label="赛事管理分区"
					defaultActiveKey="users"
					items={[
						{ key: "users", label: "参与用户", children: <EventUsersPanel eventId={eventId} /> },
						{
							key: "announcements",
							label: "赛事公告",
							children: <EventAnnouncementsPanel eventId={eventId} />,
						},
						{
							key: "instances",
							label: "实例",
							children: <EventInstancesPanel eventId={eventId} />,
						},
					]}
				/>
			</Card>

			<Modal
				open={report !== null}
				title="赛事报告 / Writeup 导出"
				typewriter={false}
				width={760}
				onClose={() => setReport(null)}
				footer={<Button onClick={() => setReport(null)}>关闭</Button>}
			>
				{/* 报告是纯文本（Markdown/表格），用等宽块展示便于复制。 */}
				<div className="ai-terminal" style={{ maxHeight: 460 }}>
					{report}
				</div>
			</Modal>
		</>
	);
}
