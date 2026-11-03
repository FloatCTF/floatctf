/**
 * 控制台总览 —— 管理员的落地页。
 *
 * 设计取舍：这一页**只回答"现在有没有事要我处理"**，不做数据大屏。
 * 因此顺序是：需要关注（失败任务 / 24h 错误日志 / AWD 告警）→ 平台规模 → 最近的动静。
 * 把"需要关注"放在最上面，是因为它是唯一需要立刻行动的部分。
 */
import { Link } from "react-router";
import { Avatar, Button, Card, Divider, Tag } from "animal-island-ui";
import {
	AlertTriangleIcon,
	BellIcon,
	CalendarIcon,
	ChatIcon,
	GridIcon,
	InboxIcon,
	MagnetIcon,
	RefreshIcon,
	ShieldIcon,
	UsersIcon,
} from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniQuery } from "~/api/hooks";
import { PageHeader } from "~/app/shell";
import { StatCard } from "~/components/resource";
import { EmptyState, ErrorState, SkeletonCards } from "~/components/states";
import { FAMILY_LABEL, formatTime } from "~/lib/event";
import type { EventFamily } from "@floatctf/sdk/entity";

export function ConsoleHome() {
	const runtime = useRuntime();
	const query = useUniQuery(
		["console", "dashboard"],
		() => runtime.client.admin.dashboard.summary(),
		"加载总览",
	);

	if (query.isLoading) return <SkeletonCards count={8} />;
	if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />;
	if (!query.data) {
		return <ErrorState error={new Error("后端没有返回总览数据")} onRetry={() => void query.refetch()} />;
	}

	const d = query.data;
	const problems =
		d.attention.failed_tasks.length + (d.attention.error_logs_24h > 0 ? 1 : 0) + d.attention.awd_alerts.length;

	return (
		<>
			<PageHeader
				title="控制台"
				description="平台规模、需要关注的问题，以及最近的动静。"
				extra={
					<Button
						size="small"
						icon={<RefreshIcon size={16} />}
						onClick={() => void query.refetch()}
					>
						刷新
					</Button>
				}
			/>

			{/* 「需要关注」永远排在最前：它是这一页里唯一要求行动的部分。 */}
			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<div className="ai-row-between">
						<span className="ai-strong" style={{ fontSize: 18 }}>
							需要关注
						</span>
						{problems === 0 ? (
							<Tag size="small" variant="solid" color="app-green">
								一切正常
							</Tag>
						) : (
							<Tag size="small" variant="solid" color="app-red">
								{problems} 项
							</Tag>
						)}
					</div>
					<Divider />

					{problems === 0 ? (
						<EmptyState
							title="没有需要处理的问题"
							description="失败任务、错误日志与 AWD 告警都清空了。"
						/>
					) : (
						<div className="ai-col" style={{ gap: 10 }}>
							{d.attention.failed_tasks.map((t) => (
								<div key={t.task_key} className="ai-row-between">
									<div className="ai-row" style={{ gap: 8 }}>
										<AlertTriangleIcon size={16} />
										<span className="ai-strong">{t.task_name}</span>
										<Tag size="small" variant="soft" color="app-red">
											第 {t.attempt_count}/{t.max_attempts} 次
										</Tag>
									</div>
									<span className="ai-muted" style={{ fontSize: 12 }}>
										{t.error_msg ?? "（无错误信息）"}
									</span>
								</div>
							))}

							{d.attention.error_logs_24h > 0 ? (
								<div className="ai-row-between">
									<div className="ai-row" style={{ gap: 8 }}>
										<AlertTriangleIcon size={16} />
										<span className="ai-strong">最近 24 小时错误日志</span>
										<Tag size="small" variant="soft" color="app-red">
											{d.attention.error_logs_24h} 条
										</Tag>
									</div>
									<Link to="/console/logs" className="ai-muted">
										去查看
									</Link>
								</div>
							) : null}

							{d.attention.awd_alerts.map((a) => (
								<div key={a.event_id} className="ai-row-between">
									<div className="ai-row" style={{ gap: 8 }}>
										<ShieldIcon size={16} />
										<span className="ai-strong">{a.title}</span>
										<Tag size="small" variant="soft" color="app-orange">
											{a.phase}
										</Tag>
									</div>
									<Link to={`/console/events/${a.event_id}`} className="ai-muted">
										去处理
									</Link>
								</div>
							))}
						</div>
					)}
				</div>
			</Card>

			<div className="ai-grid ai-grid-wide">
				<StatCard label="账号" value={d.stats.users} icon={<UsersIcon size={18} />} />
				<StatCard label="赛事" value={d.stats.events} icon={<CalendarIcon size={18} />} />
				<StatCard label="题目" value={d.stats.challenges} icon={<MagnetIcon size={18} />} />
				<StatCard label="箱子" value={d.stats.gameboxes} icon={<GridIcon size={18} />} />
				<StatCard label="武器" value={d.stats.weapons} icon={<ShieldIcon size={18} />} />
				<StatCard label="公告" value={d.stats.announcements} icon={<BellIcon size={18} />} />
				<StatCard label="讨论" value={d.stats.discussions} icon={<ChatIcon size={18} />} />
				<StatCard label="运行实例" value={d.stats.instances} icon={<InboxIcon size={18} />} />
			</div>

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<span className="ai-strong" style={{ fontSize: 18 }}>
						赛事状态
					</span>
					<Divider />
					{d.events.length === 0 ? (
						<EmptyState title="还没有赛事" description="去「赛事管理」创建第一场。" />
					) : (
						<div className="ai-scroll">
							<table style={{ width: "100%", borderCollapse: "collapse" }}>
								<thead>
									<tr>
										{["赛事", "赛制", "时间", "状态"].map((h) => (
											<th
												key={h}
												style={{
													textAlign: "left",
													padding: "8px 12px",
													borderBottom: "2px solid var(--animal-border-color)",
												}}
											>
												{h}
											</th>
										))}
									</tr>
								</thead>
								<tbody>
									{d.events.map((e) => (
										<tr key={e.event_id}>
											<td style={{ padding: "8px 12px" }}>
												<Link to={`/console/events/${e.event_id}`}>{e.title}</Link>
											</td>
											<td style={{ padding: "8px 12px" }}>
												<Tag size="small" variant="outlined">
													{FAMILY_LABEL[e.family as EventFamily] ?? e.family}
												</Tag>
											</td>
											<td style={{ padding: "8px 12px" }} className="ai-muted">
												{formatTime(e.start_time)}
											</td>
											<td style={{ padding: "8px 12px" }}>
												{e.awd ? (
													<Tag size="small" variant="soft" color="app-orange">
														AWD {e.awd.phase}
													</Tag>
												) : e.hidden ? (
													<Tag size="small" variant="soft" color="brown">
														未公开
													</Tag>
												) : (
													<Tag size="small" variant="soft" color="app-teal">
														正常
													</Tag>
												)}
											</td>
										</tr>
									))}
								</tbody>
							</table>
						</div>
					)}
				</div>
			</Card>

			<div className="ai-grid">
				<Card>
					<div className="ai-col" style={{ gap: 10 }}>
						<span className="ai-strong">最近解题</span>
						<Divider />
						{d.activity.recent_solves.length === 0 ? (
							<EmptyState title="还没有解题记录" />
						) : (
							d.activity.recent_solves.map((s, i) => (
								<div key={`${s.nickname}-${s.solved_at}-${i}`} className="ai-row-between">
									<div className="ai-row" style={{ gap: 8 }}>
										<Avatar size="small" src={s.avatar ?? undefined}>
											{s.nickname?.[0] ?? "?"}
										</Avatar>
										<span>{s.nickname}</span>
									</div>
									<div className="ai-col" style={{ alignItems: "flex-end", gap: 0 }}>
										<span className="ai-strong">{s.challenge_name}</span>
										<span className="ai-muted" style={{ fontSize: 12 }}>
											{formatTime(s.solved_at)}
										</span>
									</div>
								</div>
							))
						)}
					</div>
				</Card>

				<Card>
					<div className="ai-col" style={{ gap: 10 }}>
						<span className="ai-strong">最近注册</span>
						<Divider />
						{d.activity.recent_signups.length === 0 ? (
							<EmptyState title="还没有新注册" />
						) : (
							d.activity.recent_signups.map((s) => (
								<div key={s.username} className="ai-row-between">
									<div className="ai-row" style={{ gap: 8 }}>
										<Avatar size="small" src={s.avatar ?? undefined}>
											{s.nickname?.[0] ?? "?"}
										</Avatar>
										<span>{s.nickname}</span>
									</div>
									<span className="ai-muted">{formatTime(s.created_at)}</span>
								</div>
							))
						)}
					</div>
				</Card>
			</div>
		</>
	);
}
