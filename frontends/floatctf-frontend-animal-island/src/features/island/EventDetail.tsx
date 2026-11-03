/**
 * 赛事详情 —— 选手端的主战场。
 *
 * 结构：赛事头（状态 / 赛制 / 加入）+ 四个分区标签页（题目 / 公告 / 我的实例 / 队伍）。
 * 用 Tabs 而不是左导航：这四块都属于"这一场比赛"，来回切换不该改变上下文。
 *
 * 安全取舍：**不渲染 `instance.flag`**。实例 DTO 里带 `flag`，但对选手它就是答案本身，
 * 显示出来等于把题目送人。这里只展示 `content`（连接信息）与实例状态。
 */
import { useState } from "react";
import { useNavigate, useParams } from "react-router";
import {
	Button,
	Card,
	Divider,
	Input,
	Loading,
	Modal,
	Notification,
	Tabs,
	Tag,
	Tooltip,
} from "animal-island-ui";
import {
	CheckCircleIcon,
	FlagIcon,
	InfoIcon,
	PlayIcon,
	RefreshIcon,
	ShieldIcon,
	TrashIcon,
	UsersIcon,
} from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniMutation, useUniQuery } from "~/api/hooks";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { EmptyState, ErrorState, SkeletonCards } from "~/components/states";
import {
	FAMILY_LABEL,
	MODE_LABEL,
	PURPOSE_LABEL,
	STATUS_META,
	eventStatus,
	formatTime,
} from "~/lib/event";
import type { EventChallengeResult } from "@floatctf/sdk";

/** 单题提交：先确保有实例，再用实例 id 提交 flag。 */
function ChallengeRow({
	eventId,
	item,
	onSolved,
}: {
	eventId: string;
	item: EventChallengeResult;
	onSolved: () => void;
}) {
	const runtime = useRuntime();
	const [open, setOpen] = useState(false);
	const [flag, setFlag] = useState("");

	const launch = useUniMutation(
		() => runtime.client.service.events.launchSingleInstance(eventId, item.challenge.id),
		"启动实例",
	);

	// 打开提交框时**先查已有实例**，而不是直接再启动一个。
	// 这一步很关键：`launch` 会真的去起容器，反复打开提交框会制造重复实例。
	const existing = useUniQuery(
		["island", "challenge-instance", eventId, item.challenge.id],
		() => runtime.client.service.events.getChallengeInstance(eventId, item.challenge.id),
		"查询已有实例",
		{ enabled: open },
	);

	// 赛事维度的提交：带上 event_id（后端按它归属计分），
	// 而不是只发 instance_id 让后端自己猜赛事。
	const submit = useUniMutation(
		(vars: { event_id: string; instance_id: string; flag: string }) =>
			runtime.client.service.submit.submitSingle(vars),
		"提交 flag",
	);

	async function doSubmit() {
		const trimmed = flag.trim();
		if (!trimmed) return;
		try {
			// 已有实例就复用它；确实没有才新建。
			const inst = existing.data ?? (await launch.mutateAsync(undefined));
			await submit.mutateAsync({
				event_id: eventId,
				instance_id: inst.id,
				flag: trimmed,
			});
			Notification.success({ message: "提交成功", description: "这一题算你的了。" });
			setOpen(false);
			setFlag("");
			onSolved();
		} catch {
			// 失败已由 mutationCache 统一 toast；保持弹窗打开让用户改。
		}
	}

	return (
		<Card>
			<div className="ai-col" style={{ gap: 8 }}>
				<div className="ai-row-between">
					<div className="ai-row" style={{ gap: 8 }}>
						{item.solved ? <CheckCircleIcon size={18} /> : <FlagIcon size={18} />}
						<span className="ai-strong">{item.challenge.name}</span>
						<Tag size="small" variant="outlined">
							{item.challenge.category}
						</Tag>
					</div>
					<div className="ai-row" style={{ gap: 8 }}>
						<Tooltip title="当前分值（随解出人数动态变化）">
							<Tag size="small" variant="soft" color="app-teal">
								{item.current_points} 分
							</Tag>
						</Tooltip>
						<span className="ai-muted" style={{ fontSize: 12 }}>
							{item.solved_count} 队解出
						</span>
					</div>
				</div>

				{item.challenge.description ? (
					<span className="ai-muted" style={{ fontSize: 13 }}>
						{item.challenge.description}
					</span>
				) : null}

				<div className="ai-row">
					{item.solved ? (
						<Tag size="small" variant="solid" color="app-green">
							已解出（第 {item.solved_no} 名）
						</Tag>
					) : (
						<Button
							type="primary"
							size="small"
							icon={<FlagIcon size={16} />}
							onClick={() => setOpen(true)}
						>
							提交 flag
						</Button>
					)}
					{item.challenge.attachment ? (
						<Button
							size="small"
							type="default"
							onClick={() =>
								window.open(
									`/public/challenges/${item.challenge.safe_name}/${item.challenge.attachment?.name}`,
									"_blank",
									"noopener",
								)
							}
						>
							附件：{item.challenge.attachment.name}
						</Button>
					) : null}
				</div>
			</div>

			<Modal
				open={open}
				title={`提交 ${item.challenge.name}`}
				typewriter={false}
				onClose={() => setOpen(false)}
				footer={
					<>
						<Button onClick={() => setOpen(false)}>取消</Button>
						<Button
							type="primary"
							loading={submit.isPending || launch.isPending}
							onClick={() => void doSubmit()}
						>
							提交
						</Button>
					</>
				}
			>
				<div className="ai-col" style={{ gap: 10 }}>
					<span className="ai-muted">
						提交后该题即计入你的成绩；同一题重复提交不会重复计分。
					</span>
					{/* 让用户看清这次提交会落到哪个实例上。 */}
					{existing.isLoading ? (
						<span className="ai-muted" style={{ fontSize: 12 }}>
							正在查询你在这道题上的实例…
						</span>
					) : existing.data ? (
						<div className="ai-row" style={{ gap: 8 }}>
							<Tag size="small" variant="soft" color="app-green">
								复用已有实例
							</Tag>
							<span className="ai-mono" style={{ fontSize: 12 }}>
								{existing.data.status}
							</span>
						</div>
					) : (
						<Tag size="small" variant="soft" color="app-yellow">
							还没有实例，提交时会自动启动一个
						</Tag>
					)}
					<Input
						autoFocus
						placeholder="flag{...}"
						value={flag}
						onChange={(e) => setFlag(e.target.value)}
						onKeyDown={(e) => {
							if (e.key === "Enter") void doSubmit();
						}}
					/>
				</div>
			</Modal>
		</Card>
	);
}

export function EventDetail() {
	const { eventId = "" } = useParams();
	const runtime = useRuntime();
	const navigate = useNavigate();
	const confirm = useConfirm();

	const event = useUniQuery(
		["island", "event", eventId],
		() => runtime.client.service.events.get(eventId),
		"加载赛事",
	);
	const challenges = useUniQuery(
		["island", "event", eventId, "challenges"],
		() => runtime.client.service.events.fetchChallenges(eventId),
		"加载题目",
	);
	const announcements = useUniQuery(
		["island", "event", eventId, "announcements"],
		() => runtime.client.service.events.getAnnouncements(eventId),
		"加载公告",
	);
	const instances = useUniQuery(
		["island", "event", eventId, "instances"],
		() => runtime.client.service.events.getInstances(eventId),
		"加载实例",
	);

	const join = useUniMutation(
		() => runtime.client.service.events.join(eventId),
		"加入赛事",
	);
	const leave = useUniMutation(
		() => runtime.client.service.events.leave(eventId),
		"退出赛事",
	);

	const [teamName, setTeamName] = useState("");
	const createTeam = useUniMutation(
		(vars: { event_id: string; name: string }) =>
			runtime.client.service.events.createTeam(vars),
		"创建战队",
	);

	if (event.isLoading) return <Loading active tip="加载赛事…" />;
	if (event.error) return <ErrorState error={event.error} onRetry={() => void event.refetch()} />;
	if (!event.data) {
		return (
			<Card>
				<EmptyState title="赛事不存在" description="它可能已被删除，或者你没有查看权限。" />
			</Card>
		);
	}

	const info = event.data;
	const st = eventStatus(info);
	const meta = STATUS_META[st];
	const isTeam = info.event.participant_mode === "team";

	const challengeTab = (
		<div className="ai-col" style={{ gap: 12 }}>
			{challenges.isLoading ? (
				<SkeletonCards count={3} />
			) : challenges.error ? (
				<ErrorState error={challenges.error} onRetry={() => void challenges.refetch()} />
			) : (challenges.data?.length ?? 0) === 0 ? (
				<EmptyState
					title="这场赛事还没有放出题目"
					description="等管理员发布，或者先去做练习题。"
				/>
			) : (
				(challenges.data ?? []).map((item) => (
					<ChallengeRow
						key={item.challenge.id}
						eventId={eventId}
						item={item}
						onSolved={() => {
							void challenges.refetch();
							void event.refetch();
						}}
					/>
				))
			)}
		</div>
	);

	const announcementTab = (
		<div className="ai-col" style={{ gap: 12 }}>
			{announcements.isLoading ? (
				<SkeletonCards count={2} />
			) : announcements.error ? (
				<ErrorState
					error={announcements.error}
					onRetry={() => void announcements.refetch()}
				/>
			) : (announcements.data?.length ?? 0) === 0 ? (
				<EmptyState title="暂无公告" />
			) : (
				(announcements.data ?? []).map((a) => (
					<Card key={a.id}>
						<div className="ai-col" style={{ gap: 6 }}>
							<div className="ai-row-between">
								<span className="ai-strong">{a.title}</span>
								<span className="ai-muted" style={{ fontSize: 12 }}>
									{formatTime(a.created_at)}
								</span>
							</div>
							<Divider />
							<pre style={{ margin: 0, whiteSpace: "pre-wrap", fontFamily: "inherit" }}>
								{a.content}
							</pre>
						</div>
					</Card>
				))
			)}
		</div>
	);

	const instanceTab = (
		<div className="ai-col" style={{ gap: 12 }}>
			<span className="ai-muted">
				这里只显示你（或你战队）的实例，且**不会显示 flag** —— 那是答案本身。
			</span>
			{instances.isLoading ? (
				<SkeletonCards count={2} />
			) : instances.error ? (
				<ErrorState error={instances.error} onRetry={() => void instances.refetch()} />
			) : (instances.data?.length ?? 0) === 0 ? (
				<EmptyState
					title="还没有运行中的实例"
					description="需要容器的题会在你提交时自动启动一个。"
				/>
			) : (
				(instances.data ?? []).map((row) => (
					<Card key={row.id}>
						<div className="ai-col" style={{ gap: 6 }}>
							<div className="ai-row-between">
								<span className="ai-strong">{row.challenge_name}</span>
								<Tag size="small" variant="soft" color="app-teal">
									{row.instance.status}
								</Tag>
							</div>
							<span className="ai-muted" style={{ fontSize: 12 }}>
								归属：{row.user_nickname}
							</span>
							{row.instance.content ? (
								<div className="ai-terminal">{row.instance.content}</div>
							) : null}
							<span className="ai-muted" style={{ fontSize: 12 }}>
								{row.instance.destroy_at
									? `将在 ${formatTime(row.instance.destroy_at)} 回收`
									: "未设回收时间"}
							</span>
						</div>
					</Card>
				))
			)}
		</div>
	);

	const teamTab = isTeam ? (
		<div className="ai-col" style={{ gap: 12 }}>
			{info.team_result ? (
				<Card>
					<div className="ai-col" style={{ gap: 8 }}>
						<div className="ai-row-between">
							<span className="ai-strong">{info.team_result.team.name}</span>
							<Tag size="small" variant="soft" color="app-blue">
								{info.team_result.members.length} 人
							</Tag>
						</div>
						<Divider />
						{info.team_result.members.map((m) => (
							<div key={m.member.user_id} className="ai-row-between">
								{/* 后端给的是 member_name（已解析好的展示名）+ 成员实体 */}
								<span>{m.member_name}</span>
								<Tag size="small" variant="outlined">
									{m.member.role}
								</Tag>
							</div>
						))}
						<Button
							danger
							size="small"
							icon={<TrashIcon size={16} />}
							onClick={async () => {
								const ok = await confirm({
									title: "退出战队",
									consequence:
										"退出后你在本场赛事的战队归属会被清除，需要重新加入才能继续以战队身份计分。历史提交不会回滚。",
									okText: "退出战队",
									danger: true,
								});
								if (!ok) return;
								await runtime.client.service.events.quitTeam({
									event_id: eventId,
									team_id: info.team_result?.team.id ?? "",
								});
								void event.refetch();
							}}
						>
							退出战队
						</Button>
					</div>
				</Card>
			) : (
				<Card>
					<div className="ai-col" style={{ gap: 10 }}>
						<span className="ai-strong">你还没有战队</span>
						<span className="ai-muted">
							这是一场团队赛。创建一个战队，或让队长把你拉进去。
						</span>
						<Input
							placeholder="战队名称"
							value={teamName}
							onChange={(e) => setTeamName(e.target.value)}
						/>
						<Button
							type="primary"
							icon={<UsersIcon size={16} />}
							disabled={!teamName.trim()}
							loading={createTeam.isPending}
							onClick={async () => {
								await createTeam.mutateAsync({ event_id: eventId, name: teamName.trim() });
								setTeamName("");
								void event.refetch();
							}}
						>
							创建战队
						</Button>
					</div>
				</Card>
			)}
		</div>
	) : (
		<Card>
			<EmptyState title="这是个人赛" description="不需要组队，直接做题即可。" />
		</Card>
	);

	return (
		<>
			<PageHeader
				title={info.event.title}
				description={info.event.description ?? "（这场赛事没有写简介）"}
				extra={
					<>
						<Tag size="medium" variant="solid" color={meta.color}>
							{meta.label}
						</Tag>
						{info.joined ? (
							<Button
								danger
								size="small"
								onClick={async () => {
									const ok = await confirm({
										title: "退出赛事",
										consequence:
											"退出后你将从本场赛事的参与名单中移除；已提交的解题记录会保留，但不再计入排行榜。",
										okText: "退出",
										danger: true,
									});
									if (!ok) return;
									await leave.mutateAsync(undefined);
									void event.refetch();
								}}
							>
								退出赛事
							</Button>
						) : (
							<Button
								type="primary"
								size="small"
								icon={<PlayIcon size={16} />}
								loading={join.isPending}
								disabled={!info.event.allow_join}
								onClick={async () => {
									await join.mutateAsync(undefined);
									void event.refetch();
								}}
							>
								{info.event.allow_join ? "加入赛事" : "暂未开放加入"}
							</Button>
						)}
						<Button
							size="small"
							icon={<RefreshIcon size={16} />}
							onClick={() => {
								void event.refetch();
								void challenges.refetch();
								void instances.refetch();
							}}
						>
							刷新
						</Button>
					</>
				}
			/>

			<Card>
				<div className="ai-row" style={{ gap: 16, flexWrap: "wrap" }}>
					<Tooltip title="赛制决定计分方式与题目类型">
						<span className="ai-row" style={{ gap: 6 }}>
							<InfoIcon size={16} />
							<span className="ai-strong">{FAMILY_LABEL[info.event.family]}</span>
						</span>
					</Tooltip>
					<Tag size="small" variant="outlined">
						{PURPOSE_LABEL[info.event.purpose]}
					</Tag>
					<Tag size="small" variant="outlined">
						{MODE_LABEL[info.event.participant_mode]}
					</Tag>
					<span className="ai-muted">开始：{formatTime(info.event.start_time)}</span>
					<span className="ai-muted">结束：{formatTime(info.event.end_time)}</span>
					{!info.joined && !info.event.allow_join ? (
						<Tag size="small" variant="soft" color="app-yellow">
							未开放加入
						</Tag>
					) : null}
				</div>
				{info.event.rules ? (
					<>
						<Divider />
						<div className="ai-col" style={{ gap: 4 }}>
							<span className="ai-strong">规则</span>
							<pre style={{ margin: 0, whiteSpace: "pre-wrap", fontFamily: "inherit" }}>
								{info.event.rules}
							</pre>
						</div>
					</>
				) : null}
			</Card>

			<Card>
				<Tabs
					aria-label="赛事分区"
					defaultActiveKey="challenges"
					items={[
						{ key: "challenges", label: "题目", children: challengeTab },
						{ key: "announcements", label: "公告", children: announcementTab },
						{ key: "instances", label: "我的实例", children: instanceTab },
						{ key: "team", label: "队伍", children: teamTab },
					]}
				/>
			</Card>

			{info.event.family === "awdp" ? (
				<Card>
					<div className="ai-row-between">
						<div className="ai-col" style={{ gap: 2, alignItems: "flex-start" }}>
							<span className="ai-strong">这是一场 AWDP 修复赛</span>
							<span className="ai-muted" style={{ fontSize: 12 }}>
								Break 阶段找漏洞，Fix 阶段上传补丁。工作台里按阶段显示该做的事。
							</span>
						</div>
						<Button
							type="primary"
							icon={<ShieldIcon size={16} />}
							onClick={() => void navigate(`/island/awdp/${eventId}`)}
						>
							进入 AWDP 工作台
						</Button>
					</div>
				</Card>
			) : null}

			{info.event.family === "awd" ? (
				<Card>
					<div className="ai-row-between">
						<div className="ai-col" style={{ gap: 2, alignItems: "flex-start" }}>
							<span className="ai-strong">这是一场 AWD 攻防赛</span>
							<span className="ai-muted" style={{ fontSize: 12 }}>
								靶机、flag 提交、积分榜与接入配置都在专门的攻防面板里。
							</span>
						</div>
						<Button
							type="primary"
							icon={<ShieldIcon size={16} />}
							onClick={() => void navigate(`/island/awd/${eventId}`)}
						>
							进入攻防面板
						</Button>
					</div>
				</Card>
			) : null}

			<Card>
				<div className="ai-row-between">
					<span className="ai-muted">想看看现在谁在前面？</span>
					<Button
						size="small"
						icon={<FlagIcon size={16} />}
						onClick={() => void navigate(`/island/scoreboard?event=${eventId}`)}
					>
						查看排行
					</Button>
				</div>
			</Card>
		</>
	);
}
