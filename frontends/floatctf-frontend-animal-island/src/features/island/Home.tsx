/**
 * 岛屿总览 —— 选手端的落地页。
 *
 * 只回答三个问题：**我参加了什么**、**现在能做什么**、**下一步去哪**。
 * 因此刻意不堆统计数字，而是按"进行中 / 即将开始 / 已结束"分组呈现我参与的赛事。
 */
import { useMemo } from "react";
import { useNavigate } from "react-router";
import { Avatar, Button, Card, Divider, Tag } from "animal-island-ui";
import { CompassIcon, FlagIcon, MagnetIcon, TrophyIcon } from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useUniQuery } from "~/api/hooks";
import { SkeletonCards, EmptyState, ErrorState } from "~/components/states";
import { StatCard } from "~/components/resource";
import { PageHeader } from "~/app/shell";
import {
	FAMILY_COLOR,
	FAMILY_LABEL,
	STATUS_META,
	eventIdOf,
	eventStatus,
	formatTime,
	relativeTime,
} from "~/lib/event";
import type { EventInfo } from "@floatctf/sdk";

function EventCard({ info }: { info: EventInfo }) {
	const navigate = useNavigate();
	const status = eventStatus(info);
	const meta = STATUS_META[status];
	const id = eventIdOf(info);
	return (
		<Card color={FAMILY_COLOR[info.event.family]} hoverable>
			<div className="ai-col" style={{ gap: 8 }}>
				<div className="ai-row-between">
					<span className="ai-strong" style={{ fontSize: 16 }}>
						{info.event.title}
					</span>
					<Tag size="small" variant="solid" color={meta.color}>
						{meta.label}
					</Tag>
				</div>
				<div className="ai-row" style={{ gap: 6 }}>
					<Tag size="small" variant="outlined">
						{FAMILY_LABEL[info.event.family]}
					</Tag>
					{info.joined ? (
						<Tag size="small" variant="soft" color="app-green">
							已参加
						</Tag>
					) : (
						<Tag size="small" variant="soft" color="app-yellow">
							未参加
						</Tag>
					)}
				</div>
				<Divider />
				<div className="ai-col" style={{ gap: 2 }}>
					<span className="ai-muted" style={{ fontSize: 12 }}>
						{status === "upcoming"
							? `${relativeTime(info.event.start_time)}开始`
							: status === "running"
								? `开始于 ${formatTime(info.event.start_time)}`
								: `结束于 ${formatTime(info.event.end_time)}`}
					</span>
				</div>
				<Button
					size="small"
					type="primary"
					icon={<CompassIcon size={16} />}
					onClick={() => void navigate(`/island/events/${id}`)}
				>
					进入
				</Button>
			</div>
		</Card>
	);
}

export function IslandHome() {
	const runtime = useRuntime();
	const navigate = useNavigate();

	const events = useUniQuery(
		["island", "events"],
		() => runtime.client.service.events.fetch(),
		"加载赛事",
	);

	// Top15 解题榜（全平台，不分赛事）—— 与赛事列表互不影响，各自独立加载。
	const top = useUniQuery(
		["island", "top-users"],
		() => runtime.client.service.solves.getTop15Users(),
		"加载解题榜",
	);

	const { running, upcoming, ended, mine } = useMemo(() => {
		const list = events.data ?? [];
		const r: EventInfo[] = [];
		const u: EventInfo[] = [];
		const e: EventInfo[] = [];
		for (const info of list) {
			const s = eventStatus(info);
			if (s === "running") r.push(info);
			else if (s === "upcoming") u.push(info);
			else e.push(info);
		}
		return { running: r, upcoming: u, ended: e, mine: list.filter((i) => i.joined) };
	}, [events.data]);

	if (events.isLoading) return <SkeletonCards count={6} />;
	if (events.error) return <ErrorState error={events.error} onRetry={() => void events.refetch()} />;

	return (
		<>
			<PageHeader
				title="岛屿总览"
				description="你参加的赛事、正在进行中的比赛，以及下一步可以做什么。"
				extra={
					// 用 navigate 而不是把 <Link> 套进 <Button>：库的 Button 渲染的是
					// 原生 <button>，里面再放 <a> 是非法嵌套（键盘与读屏都会出问题）。
					<Button
						type="primary"
						icon={<FlagIcon size={16} />}
						onClick={() => void navigate("/island/events")}
					>
						全部赛事
					</Button>
				}
			/>

			<div className="ai-grid">
				<StatCard
					label="我参加的赛事"
					value={mine.length}
					icon={<FlagIcon size={18} />}
					hint="含已结束"
				/>
				<StatCard
					label="进行中"
					value={running.length}
					icon={<CompassIcon size={18} />}
					hint="现在就能打"
				/>
				<StatCard
					label="即将开始"
					value={upcoming.length}
					icon={<MagnetIcon size={18} />}
					hint="按开始时间排序"
				/>
			</div>

			{running.length > 0 ? (
				<Card>
					<div className="ai-col" style={{ gap: 12 }}>
						<div className="ai-row-between">
							<span className="ai-strong" style={{ fontSize: 18 }}>
								进行中
							</span>
							<Tag size="small" variant="soft" color="app-teal">
								{running.length} 场
							</Tag>
						</div>
						<div className="ai-grid">
							{running.map((i) => (
								<EventCard key={eventIdOf(i)} info={i} />
							))}
						</div>
					</div>
				</Card>
			) : null}

			<Card>
				<div className="ai-col" style={{ gap: 12 }}>
					<span className="ai-strong" style={{ fontSize: 18 }}>
						即将开始
					</span>
					{upcoming.length === 0 ? (
						<EmptyState
							title="暂时没有即将开始的赛事"
							description="去题库练习一下，或者看看排行榜。"
							action={
								<Button
									type="default"
									icon={<MagnetIcon size={16} />}
									onClick={() => void navigate("/island/challenges")}
								>
									去题库
								</Button>
							}
						/>
					) : (
						<div className="ai-grid">
							{upcoming.slice(0, 6).map((i) => (
								<EventCard key={eventIdOf(i)} info={i} />
							))}
						</div>
					)}
				</div>
			</Card>

			{ended.length > 0 ? (
				<Card>
					<div className="ai-col" style={{ gap: 12 }}>
						<div className="ai-row-between">
							<span className="ai-strong" style={{ fontSize: 18 }}>
								已结束
							</span>
							<Button
								type="text"
								size="small"
								icon={<TrophyIcon size={16} />}
								onClick={() => void navigate("/island/scoreboard")}
							>
								看排行
							</Button>
						</div>
						<div className="ai-grid">
							{ended.slice(0, 6).map((i) => (
								<EventCard key={eventIdOf(i)} info={i} />
							))}
						</div>
					</div>
				</Card>
			) : null}
			<Card>
				<div className="ai-col" style={{ gap: 10 }}>
					<div className="ai-row-between">
						<div className="ai-row" style={{ gap: 8 }}>
							<TrophyIcon size={18} />
							<span className="ai-strong" style={{ fontSize: 18 }}>
								解题榜 Top 15
							</span>
						</div>
						<span className="ai-muted" style={{ fontSize: 12 }}>
							按解出题数排序（全平台，不分赛事）
						</span>
					</div>
					<Divider />
					{top.isLoading ? (
						<SkeletonCards count={2} />
					) : top.error ? (
						<ErrorState error={top.error} onRetry={() => void top.refetch()} />
					) : (top.data ?? []).length === 0 ? (
						<EmptyState title="还没有人解出题目" description="第一个解出题的人会出现在这里。" />
					) : (
						<div className="ai-col" style={{ gap: 8 }}>
							{(top.data ?? []).map((u) => (
								<div key={u.no} className="ai-row-between">
									<div className="ai-row" style={{ gap: 10 }}>
										<Tag size="small" variant="soft" color={u.no <= 3 ? "app-yellow" : "default"}>
											#{u.no}
										</Tag>
										<Avatar size="small" src={u.avatar ?? undefined}>
											{u.nickname?.[0] ?? "?"}
										</Avatar>
										<span className="ai-strong">{u.nickname}</span>
									</div>
									<div className="ai-row" style={{ gap: 8 }}>
										<Tag size="small" variant="outlined">
											{u.solved_count} 题
										</Tag>
										<span className="ai-muted" style={{ fontSize: 12 }}>
											最近 {relativeTime(u.solved_last_at)}
										</span>
									</div>
								</div>
							))}
						</div>
					)}
				</div>
			</Card>
		</>
	);
}
