/**
 * 选手端赛事 —— 列表 + **赛事工作区**（一个赛事一个工作区，能力以标签面板呈现）。
 *
 * 参赛门禁与后端一致：非「未开始」且未加入时不展示题目 / 积分榜 / 实例等工作区内容。
 */

import { useMemo, useState } from "react";
import { Link, useParams, useSearchParams } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { CalendarDays, ChevronRight, Search } from "lucide-react";

import { call, callList } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import {
	PageBody,
	PageHeader,
	PaginationBar,
	SectionCard,
	Toolbar,
} from "~/components/app/page";
import {
	EmptyBlock,
	LoadingBlock,
	NotFoundBlock,
	QueryState,
	TableSkeleton,
} from "~/components/app/states";
import { TonePill } from "~/components/app/badges";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "~/components/ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "~/components/ui/tabs";
import type { EventChallengeResult, EventInfo } from "@floatctf/sdk";
import { formatRange } from "~/lib/format";
import {
	EVENT_FAMILY_LABEL,
	EVENT_STATUS_LABEL,
	EVENT_STATUS_TONE,
	PARTICIPANT_MODE_LABEL,
	computeEventStatus,
	eventCapabilities,
	eventIdOf,
} from "~/lib/event-status";
import { useDebouncedValue, useDocumentTitle, useNow } from "~/lib/hooks";

import {
	EventStatusPill,
	JoinLeaveControl,
	MemberGateNotice,
	TeamPanel,
} from "./components";
import { ChallengeWorkbench } from "~/features/jeopardy/workbench";
import {
	ArenaSummaryTab,
	BulletinTab,
	ChallengesTab,
	InstancesTab,
	LabSummaryTab,
	OverviewTab,
	ScoreboardTab,
	TrendTab,
} from "./tabs";

/* ── 赛事列表 ───────────────────────────────────────────────────────────── */

export function EventsListPage() {
	useDocumentTitle("赛事 · FloatCTF");
	const client = useClient();
	const now = useNow(30_000);
	const [search, setSearch] = useState("");
	const [family, setFamily] = useState("all");
	const [statusFilter, setStatusFilter] = useState("all");
	const [onlyJoined, setOnlyJoined] = useState(false);
	const [page, setPage] = useState(1);
	const [pageSize, setPageSize] = useState(20);
	const debouncedSearch = useDebouncedValue(search, 300);

	const query = useQuery({
		queryKey: qk.events.list({ limit: 200 }),
		queryFn: () => callList<EventInfo>(client.service.events.fetch({ limit: 200 })),
	});

	const items = query.data?.items ?? [];

	const filtered = useMemo(() => {
		const keyword = debouncedSearch.trim().toLowerCase();
		return items.filter((item) => {
			const status = computeEventStatus(
				item.event.start_time,
				item.event.end_time ?? null,
				now.getTime(),
			);
			if (family !== "all" && item.event.family !== family) return false;
			if (statusFilter !== "all" && status !== statusFilter) return false;
			if (onlyJoined && !item.joined) return false;
			if (keyword === "") return true;
			return (
				item.event.title.toLowerCase().includes(keyword) ||
				(item.event.description ?? "").toLowerCase().includes(keyword)
			);
		});
	}, [items, family, statusFilter, onlyJoined, debouncedSearch, now]);

	const sorted = useMemo(
		() =>
			[...filtered].sort(
				(a, b) => new Date(b.event.start_time).getTime() - new Date(a.event.start_time).getTime(),
			),
		[filtered],
	);
	const paged = sorted.slice((page - 1) * pageSize, page * pageSize);

	const columns: DataTableColumn<EventInfo>[] = [
		{
			id: "title",
			header: "赛事",
			sortValue: (row) => row.event.title,
			cell: (row) => (
				<div className="min-w-0">
					<Link to={`/events/${eventIdOf(row)}`} className="truncate text-sm font-medium hover:underline">
						{row.event.title}
					</Link>
					<p className="truncate text-xs text-muted-foreground">
						{row.event.purpose === "practice" ? "练习" : "正式赛"} ·{" "}
						{PARTICIPANT_MODE_LABEL[row.event.participant_mode] ?? row.event.participant_mode}
					</p>
				</div>
			),
		},
		{
			id: "family",
			header: "家族",
			sortValue: (row) => row.event.family,
			cell: (row) => (
				<TonePill tone="neutral">{EVENT_FAMILY_LABEL[row.event.family] ?? row.event.family}</TonePill>
			),
		},
		{
			id: "status",
			header: "状态",
			sortValue: (row) =>
				computeEventStatus(row.event.start_time, row.event.end_time ?? null, now.getTime()),
			cell: (row) => <EventStatusPill event={row.event} />,
		},
		{
			id: "window",
			header: "时间窗",
			hideBelow: "lg",
			cell: (row) => (
				<span className="text-xs text-muted-foreground">
					{formatRange(row.event.start_time, row.event.end_time ?? null)}
				</span>
			),
		},
		{
			id: "joined",
			header: "参赛",
			cell: (row) =>
				row.joined ? (
					<TonePill tone="success">已加入</TonePill>
				) : (
					<TonePill tone="muted">未加入</TonePill>
				),
		},
		{
			id: "action",
			header: "",
			align: "right",
			cell: (row) => (
				<Button variant="ghost" size="sm" asChild>
					<Link to={`/events/${eventIdOf(row)}`}>
						打开 <ChevronRight />
					</Link>
				</Button>
			),
		},
	];

	return (
		<PageBody>
			<PageHeader
				title="赛事"
				description="全部可见赛事。加入后进入赛事工作区解题 / 攻防。"
			/>

			<SectionCard
				title="赛事列表"
				description={query.isSuccess ? `共 ${items.length} 场，筛选后 ${filtered.length} 场` : undefined}
				actions={
					<Toolbar>
						<div className="relative">
							<Search className="pointer-events-none absolute top-1/2 left-2 size-4 -translate-y-1/2 text-muted-foreground" />
							<Input
								value={search}
								onChange={(event) => {
									setSearch(event.target.value);
									setPage(1);
								}}
								placeholder="搜索赛事标题"
								className="w-52 pl-8"
							/>
						</div>
						<Select
							value={family}
							onValueChange={(value) => {
								setFamily(value);
								setPage(1);
							}}
						>
							<SelectTrigger className="w-40" aria-label="家族筛选">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value="all">全部家族</SelectItem>
								<SelectItem value="jeopardy">Jeopardy 解题</SelectItem>
								<SelectItem value="awd">AWD 攻防</SelectItem>
								<SelectItem value="awdp">AWDP 攻防</SelectItem>
							</SelectContent>
						</Select>
						<Select
							value={statusFilter}
							onValueChange={(value) => {
								setStatusFilter(value);
								setPage(1);
							}}
						>
							<SelectTrigger className="w-32" aria-label="状态筛选">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value="all">全部状态</SelectItem>
								<SelectItem value="upcoming">未开始</SelectItem>
								<SelectItem value="ongoing">进行中</SelectItem>
								<SelectItem value="ended">已结束</SelectItem>
							</SelectContent>
						</Select>
						<Button
							variant={onlyJoined ? "default" : "outline"}
							size="sm"
							onClick={() => {
								setOnlyJoined((value) => !value);
								setPage(1);
							}}
						>
							只看我参加的
						</Button>
					</Toolbar>
				}
				contentClassName="px-0"
				footer={
					filtered.length > pageSize ? (
						<PaginationBar
							page={page}
							pageSize={pageSize}
							total={filtered.length}
							onPageChange={setPage}
							onPageSizeChange={(size) => {
								setPageSize(size);
								setPage(1);
							}}
						/>
					) : null
				}
			>
				<QueryState
					query={query}
					skeleton={<TableSkeleton rows={6} columns={5} />}
					errorTitle="加载赛事失败"
					isEmpty={() => items.length === 0}
					empty={
						<EmptyBlock
							title="还没有可见的赛事"
							description="管理员创建并公开赛事后会出现在这里。"
							icon={<CalendarDays className="size-5" />}
						/>
					}
				>
					{() =>
						filtered.length === 0 ? (
							<EmptyBlock variant="filtered" />
						) : (
							<DataTable
								data={paged}
								getRowId={(row) => eventIdOf(row)}
								columns={columns}
								mobileCard={(row) => (
									<Link to={`/events/${eventIdOf(row)}`} className="block space-y-1">
										<div className="flex items-center justify-between gap-2">
											<span className="truncate text-sm font-medium">{row.event.title}</span>
											<EventStatusPill event={row.event} />
										</div>
										<p className="text-xs text-muted-foreground">
											{EVENT_FAMILY_LABEL[row.event.family] ?? row.event.family} ·{" "}
											{row.joined ? "已加入" : "未加入"}
										</p>
									</Link>
								)}
							/>
						)
					}
				</QueryState>
			</SectionCard>
		</PageBody>
	);
}

/* ── 赛事工作区 ─────────────────────────────────────────────────────────── */

export function EventWorkspacePage() {
	const { eventId = "" } = useParams<{ eventId: string }>();
	const client = useClient();
	const now = useNow(30_000);
	const [params, setParams] = useSearchParams();
	const tab = params.get("tab") ?? "overview";

	const query = useQuery({
		queryKey: qk.events.detail(eventId),
		queryFn: () => call<EventInfo>(client.service.events.get(eventId), "赛事详情"),
		enabled: eventId !== "",
	});

	const info = query.data;
	useDocumentTitle(info ? `${info.event.title} · FloatCTF` : "赛事 · FloatCTF");

	return (
		<PageBody>
			<QueryState
				query={query}
				skeleton={<LoadingBlock label="加载赛事…" />}
				errorTitle="加载赛事失败"
			>
				{(data) => {
					const status = computeEventStatus(
						data.event.start_time,
						data.event.end_time ?? null,
						now.getTime(),
					);
					const caps = eventCapabilities(data.event);
					const eventId = eventIdOf(data);
					const gated = status !== "upcoming" && !data.joined;
					const isTeam = data.event.participant_mode === "team";

					return (
						<>
							<PageHeader
								title={data.event.title}
								description={formatRange(data.event.start_time, data.event.end_time ?? null)}
								badge={
									<div className="flex flex-wrap items-center gap-2">
										<EventStatusPill event={data.event} />
										<TonePill tone="neutral">
											{EVENT_FAMILY_LABEL[data.event.family] ?? data.event.family}
										</TonePill>
										<TonePill tone="neutral">
											{PARTICIPANT_MODE_LABEL[data.event.participant_mode] ??
												data.event.participant_mode}
										</TonePill>
										{data.joined ? <TonePill tone="success">已参赛</TonePill> : null}
									</div>
								}
								actions={
									<Button variant="outline" asChild>
										<Link to="/events">返回赛事列表</Link>
									</Button>
								}
							/>

							<div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_340px]">
								<div className="min-w-0 space-y-5">
									{gated ? (
										<MemberGateNotice />
									) : (
										<Tabs
											value={tab}
											onValueChange={(value) => {
												params.set("tab", value);
												setParams(params, { replace: true });
											}}
										>
											<TabsList className="no-scrollbar flex w-full justify-start overflow-x-auto">
												<TabsTrigger value="overview">总览</TabsTrigger>
												{caps.challenges ? (
													<TabsTrigger value="challenges">题目</TabsTrigger>
												) : null}
												{caps.arena ? <TabsTrigger value="arena">AWD</TabsTrigger> : null}
												{caps.lab ? <TabsTrigger value="lab">AWDP</TabsTrigger> : null}
												{caps.scoreboard ? (
													<TabsTrigger value="scoreboard">积分榜</TabsTrigger>
												) : null}
												{caps.trend ? <TabsTrigger value="trend">趋势</TabsTrigger> : null}
												{caps.instances ? (
													<TabsTrigger value="instances">实例</TabsTrigger>
												) : null}
												<TabsTrigger value="bulletin">公告</TabsTrigger>
											</TabsList>

											<TabsContent value="overview" className="mt-4">
												<OverviewTab info={data} />
											</TabsContent>
											{caps.challenges ? (
												<TabsContent value="challenges" className="mt-4">
													<ChallengesTab eventId={eventId} />
												</TabsContent>
											) : null}
											{caps.arena ? (
												<TabsContent value="arena" className="mt-4">
													<ArenaSummaryTab eventId={eventId} />
												</TabsContent>
											) : null}
											{caps.lab ? (
												<TabsContent value="lab" className="mt-4">
													<LabSummaryTab eventId={eventId} />
												</TabsContent>
											) : null}
											{caps.scoreboard ? (
												<TabsContent value="scoreboard" className="mt-4">
													<ScoreboardTab info={data} />
												</TabsContent>
											) : null}
											{caps.trend ? (
												<TabsContent value="trend" className="mt-4">
													<TrendTab eventId={eventId} />
												</TabsContent>
											) : null}
											{caps.instances ? (
												<TabsContent value="instances" className="mt-4">
													<InstancesTab eventId={eventId} />
												</TabsContent>
											) : null}
											<TabsContent value="bulletin" className="mt-4">
												<BulletinTab eventId={eventId} />
											</TabsContent>
										</Tabs>
									)}
								</div>

								<div className="space-y-5">
									<SectionCard
										title={isTeam ? "我的战队" : "参赛"}
										description={
											status === "upcoming"
												? "赛事开始前可自由加入 / 退出。"
												: "赛事进行中，参赛名单已锁定。"
										}
									>
										{isTeam ? <TeamPanel info={data} /> : <JoinLeaveControl info={data} />}
									</SectionCard>

									{!gated ? (
										<SectionCard title="快速跳转">
											<div className="flex flex-col gap-2">
												{caps.challenges ? (
													<Button variant="outline" asChild className="justify-start">
														<Link to={`/events/${eventId}?tab=challenges`}>赛事题目列表</Link>
													</Button>
												) : null}
												{caps.scoreboard ? (
													<Button variant="outline" asChild className="justify-start">
														<Link to={`/events/${eventId}?tab=scoreboard`}>积分榜</Link>
													</Button>
												) : null}
												{caps.arena ? (
													<Button variant="outline" asChild className="justify-start">
														<Link to={`/arena/${eventId}`}>AWD 驾驶舱</Link>
													</Button>
												) : null}
												{caps.lab ? (
													<Button variant="outline" asChild className="justify-start">
														<Link to={`/lab/${eventId}`}>AWDP 工作台</Link>
													</Button>
												) : null}
											</div>
										</SectionCard>
									) : null}
								</div>
							</div>
						</>
					);
				}}
			</QueryState>
		</PageBody>
	);
}

/* ── 赛事内题目工作台 ───────────────────────────────────────────────────── */

export function EventChallengePage() {
	const { eventId = "", challengeId = "" } = useParams<{ eventId: string; challengeId: string }>();
	const client = useClient();

	const eventQuery = useQuery({
		queryKey: qk.events.detail(eventId),
		queryFn: () => call<EventInfo>(client.service.events.get(eventId), "赛事详情"),
		enabled: eventId !== "",
	});

	const challengesQuery = useQuery({
		queryKey: qk.events.challenges(eventId, { limit: 200 }),
		queryFn: () => callList<EventChallengeResult>(client.service.events.fetchChallenges(eventId, { limit: 200 })),
		enabled: eventId !== "",
	});

	const item = (challengesQuery.data?.items ?? []).find(
		(entry) => entry.challenge.id === challengeId,
	);

	return (
		<PageBody>
			<QueryState
				query={challengesQuery}
				skeleton={<LoadingBlock label="加载题目…" />}
				errorTitle="加载赛事题目失败"
			>
				{() =>
					item ? (
						<>
							<PageHeader
								title={item.challenge.name}
								description={
									<span className="flex flex-wrap items-center gap-2">
										<TonePill tone="info">当前分值 {item.current_points}</TonePill>
										<TonePill tone="neutral">{item.solved_count} 队解出</TonePill>
										{item.solved ? (
											<TonePill tone="success">已解出 #{item.solved_no}</TonePill>
										) : null}
									</span>
								}
								breadcrumbs={
									<span className="flex flex-wrap items-center gap-1">
										<Link to="/events" className="hover:underline">
											赛事
										</Link>
										<ChevronRight className="size-3" />
										<Link to={`/events/${eventId}`} className="hover:underline">
											{eventQuery.data?.event.title ?? eventId}
										</Link>
										<ChevronRight className="size-3" />
										<span>{item.challenge.category || "未分类"}</span>
									</span>
								}
								actions={
									<Button variant="outline" asChild>
										<Link to={`/events/${eventId}?tab=challenges`}>返回题目列表</Link>
									</Button>
								}
							/>
							<ChallengeWorkbench
								challenge={item.challenge}
								mode={{ kind: "event", eventId }}
							/>
						</>
					) : (
						<NotFoundBlock
							title="该题目不在本赛事中"
							description="题目可能已被下架，或赛事题目列表已更新。"
							action={
								<Button asChild>
									<Link to={`/events/${eventId}?tab=challenges`}>返回题目列表</Link>
								</Button>
							}
						/>
					)
				}
			</QueryState>
		</PageBody>
	);
}

export { EVENT_STATUS_LABEL, EVENT_STATUS_TONE };
