/**
 * 选手总览（Field 首页）—— 用**真实接口**回答四个问题：
 * 我在哪些赛事里？平台在发生什么？我的下一步是什么？公告有哪些？
 */

import type { ReactNode } from "react";
import { Link, useNavigate } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { ArrowRight, CalendarDays, Megaphone, Puzzle, Trophy } from "lucide-react";

import { call, callList } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { useAuthStore, useMe } from "~/auth/store";
import { TonePill } from "~/components/app/badges";
import { UserAvatar } from "~/components/app/user-cell";
import {
	PageBody,
	PageHeader,
	SectionCard,
	StatCard,
	Toolbar,
} from "~/components/app/page";
import {
	EmptyBlock,
	ErrorBlock,
	LoadingBlock,
	QueryState,
} from "~/components/app/states";
import { Button } from "~/components/ui/button";
import { formatInt, formatRelative, formatScore } from "~/lib/format";
import {
	EVENT_FAMILY_LABEL,
	EVENT_STATUS_LABEL,
	EVENT_STATUS_TONE,
	eventStatusOf,
} from "~/lib/event-status";
import { useDocumentTitle, useNow } from "~/lib/hooks";

export function DashboardPage(): ReactNode {
	useDocumentTitle("总览 · FloatCTF");
	const client = useClient();
	const me = useMe();
	const userToken = useAuthStore((state) => state.userToken);
	const navigate = useNavigate();
	const now = useNow(30_000);

	const events = useQuery({
		queryKey: qk.events.list({ limit: 50 }),
		queryFn: () => callList(client.service.events.fetch({ limit: 50 })),
		enabled: userToken !== null,
	});

	const announcements = useQuery({
		queryKey: qk.announcements.list({ limit: 5 }),
		queryFn: () => callList(client.service.announcements.fetch({ limit: 5 })),
		refetchInterval: 60_000,
	});

	const solves = useQuery({
		queryKey: qk.solves.list({ limit: 8 }),
		queryFn: () => callList(client.service.solves.fetch({ limit: 8 })),
		refetchInterval: 30_000,
	});

	const top = useQuery({
		queryKey: qk.solves.top15(),
		queryFn: () => call(client.service.solves.getTop15Users(), "Top15 排行"),
		refetchInterval: 30_000,
	});

	const myEvents = (events.data?.items ?? []).filter((item) => item.joined);
	const openEvents = (events.data?.items ?? []).filter((item) => !item.joined);

	return (
		<PageBody>
			<PageHeader
				title={me ? `你好，${me.nickname || me.username}` : "总览"}
				description="你的赛事进度、平台动态与下一步动作。"
				actions={
					<Toolbar>
						<Button variant="outline" asChild>
							<Link to="/events">
								<CalendarDays /> 全部赛事
							</Link>
						</Button>
						<Button asChild>
							<Link to="/challenges">
								<Puzzle /> 去解题
							</Link>
						</Button>
					</Toolbar>
				}
			/>

			<div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
				<StatCard
					label="我参加的赛事"
					value={events.isPending ? "…" : formatInt(myEvents.length)}
					hint={openEvents.length > 0 ? `另有 ${openEvents.length} 场可加入` : "暂无新的可加入赛事"}
					icon={<CalendarDays className="size-5" />}
				/>
				<StatCard
					label="进行中的赛事"
					value={events.isPending ? "…" : formatInt(myEvents.filter((item) => eventStatusOf(item.event, now.getTime()) === "ongoing").length)}
					hint="按赛事时间窗判定"
					icon={<Trophy className="size-5" />}
					tone="success"
				/>
				<StatCard
					label="我的练习解题"
					value={solves.isPending ? "…" : formatInt(solves.data?.meta?.total ?? solves.data?.items.length ?? 0)}
					hint="题库练习记录（后端按当前账号返回，不含赛事）"
					icon={<Puzzle className="size-5" />}
				/>
				<StatCard
					label="公告"
					value={announcements.isPending ? "…" : formatInt(announcements.data?.meta?.total ?? announcements.data?.items.length ?? 0)}
					hint="全站级通知"
					icon={<Megaphone className="size-5" />}
				/>
			</div>

			<div className="grid gap-5 lg:grid-cols-2">
				<SectionCard
					title="我的赛事"
					description="已加入的赛事，点开进入赛事工作区。"
					actions={
						<Button variant="ghost" size="sm" asChild>
							<Link to="/events">
								全部 <ArrowRight />
							</Link>
						</Button>
					}
				>
					<QueryState
						query={events}
						skeleton={<LoadingBlock label="加载赛事…" />}
						isEmpty={() => myEvents.length === 0}
						empty={
							<EmptyBlock
								title="还没有加入任何赛事"
								description="在赛事列表里加入一场比赛后，这里会显示你的进度。"
								action={
									<Button size="sm" asChild>
										<Link to="/events">浏览赛事</Link>
									</Button>
								}
							/>
						}
					>
						{() => (
							<ul className="divide-y">
								{myEvents.map((item) => {
									const status = eventStatusOf(item.event, now.getTime());
									return (
										<li key={item.id}>
											<button
												type="button"
												className="flex w-full items-center justify-between gap-3 py-3 text-left hover:bg-muted/40"
												onClick={() => void navigate(`/events/${item.id}`)}
											>
												<div className="min-w-0">
													<p className="truncate text-sm font-medium">{item.event.title}</p>
													<p className="mt-0.5 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
														<span>{EVENT_FAMILY_LABEL[item.event.family] ?? item.event.family}</span>
														<span>·</span>
														<span>{formatRelative(item.event.start_time, now)}开始</span>
													</p>
												</div>
												<TonePill tone={EVENT_STATUS_TONE[status]}>
													{EVENT_STATUS_LABEL[status]}
												</TonePill>
											</button>
										</li>
									);
								})}
							</ul>
						)}
					</QueryState>
				</SectionCard>

				<SectionCard title="全站公告" description="平台与赛事通知。">
					<QueryState
						query={announcements}
						skeleton={<LoadingBlock label="加载公告…" />}
						isEmpty={(result) => result.items.length === 0}
						empty={<EmptyBlock title="暂无公告" />}
					>
						{(result) => (
							<ul className="space-y-3">
								{result.items.map((item) => (
									<li key={item.id} className="rounded-md border p-3">
										<p className="text-sm font-medium">{item.title}</p>
										<p className="mt-1 line-clamp-3 text-sm text-muted-foreground">
											{item.content}
										</p>
										<p className="mt-1 text-xs text-muted-foreground">
											{formatRelative(item.created_at, now)}
										</p>
									</li>
								))}
							</ul>
						)}
					</QueryState>
				</SectionCard>

				<SectionCard
					title="最近练习解题"
					description="你在题库练习中的解题记录（30s 轮询；赛事解题不计入此接口）。"
					actions={
						<Button variant="ghost" size="sm" asChild>
							<Link to="/solves">
								全部 <ArrowRight />
							</Link>
						</Button>
					}
				>
					<QueryState
						query={solves}
						skeleton={<LoadingBlock label="加载解题记录…" />}
						errorTitle="加载解题记录失败"
						isEmpty={(result) => result.items.length === 0}
						empty={<EmptyBlock title="还没有解题记录" />}
					>
						{(result) => (
							<ul className="divide-y">
								{result.items.map((solve) => (
									<li key={solve.id} className="flex items-center gap-3 py-2.5">
										<UserAvatar name={solve.nickname} avatar={solve.avatar} size="sm" />
										<div className="min-w-0 flex-1">
											<p className="truncate text-sm">
												<span className="font-medium">{solve.nickname}</span>
												<span className="text-muted-foreground"> 解出 </span>
												<span>{solve.challenge_name}</span>
											</p>
											<p className="text-xs text-muted-foreground">
												{formatRelative(solve.created_at, now)}
											</p>
										</div>
										<span className="tnum font-mono text-xs text-muted-foreground">
											+{formatScore(solve.obtained_points)}
										</span>
									</li>
								))}
							</ul>
						)}
					</QueryState>
				</SectionCard>

				<SectionCard
					title="Top15 排行"
					description="系统练习赛事的解题数排名（后端固定返回最多 15 条）。"
					actions={
						<Button variant="ghost" size="sm" asChild>
							<Link to="/rank">
								完整榜单 <ArrowRight />
							</Link>
						</Button>
					}
				>
					{top.isPending ? (
						<LoadingBlock label="加载排行…" />
					) : top.isError ? (
						<ErrorBlock error={top.error} title="加载排行失败" onRetry={() => top.refetch()} />
					) : (top.data ?? []).length === 0 ? (
						<EmptyBlock title="暂无排行数据" />
					) : (
						<ol className="divide-y">
							{(top.data ?? []).slice(0, 8).map((user) => (
								<li key={`${user.no}-${user.nickname}`} className="flex items-center gap-3 py-2.5">
									<span className="tnum w-6 text-center font-mono text-xs text-muted-foreground">
										{user.no}
									</span>
									<UserAvatar name={user.nickname} avatar={user.avatar} size="sm" />
									<span className="min-w-0 flex-1 truncate text-sm">{user.nickname}</span>
									<span className="tnum font-mono text-xs">{formatInt(user.solved_count)} 题</span>
								</li>
							))}
						</ol>
					)}
				</SectionCard>
			</div>
		</PageBody>
	);
}
