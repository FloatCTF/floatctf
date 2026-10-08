/**
 * `/admin/events/:eventId` —— 赛事控制台（一个赛事 = 一个工作区）。
 *
 * 头部：标题 / 状态 / 时间窗 / 家族 / 赛制 + 快捷操作；正文：`<Tabs>` 标签面板，
 * 每个标签只负责一个域的数据与操作（配置 / 题目 / 成员 / 战队 / 公告 / 实例 / 日志 /
 * Writeup / 数据大屏 / AWD 概览 / AWDP 概览）。
 *
 * 标签状态写进 `?tab=`，因此控制台里的任何视图都可以直接分享 / 刷新保持。
 */

import type { ReactNode } from "react";
import { Link, useParams, useSearchParams } from "react-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { RefreshCw, Swords, Terminal } from "lucide-react";

import { call } from "~/api/call";
import { useClient } from "~/api/client";
import { isForbidden, isNotFound } from "~/api/errors";
import { qk } from "~/api/keys";
import { CopyText, PageBody, PageHeader, Toolbar } from "~/components/app/page";
import {
	ErrorBlock,
	LoadingBlock,
	NotFoundBlock,
	PermissionDeniedBlock,
} from "~/components/app/states";
import { Button } from "~/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "~/components/ui/tabs";
import type { Events } from "@floatctf/sdk/entity";
import { formatRange } from "~/lib/format";
import { useDocumentTitle } from "~/lib/hooks";

import { EventConfigTab } from "./tab-config";
import { EventChallengesTab } from "./tab-challenges";
import { EventMembersTab } from "./tab-members";
import { EventTeamsTab } from "./tab-teams";
import { EventAnnouncementsTab } from "./tab-announcements";
import { EventInstancesTab } from "./tab-instances";
import { EventLogsTab, EventWriteupsTab } from "./tab-records";
import { EventDataTab } from "./tab-data";
import { EventAwdpTab, EventAwdTab } from "./tab-ops";
import { EventFamilyPill, EventPurposePills, EventStatusPill, ParticipantModePill } from "./shared";

const TAB_KEYS = [
	"config",
	"challenges",
	"members",
	"teams",
	"announcements",
	"instances",
	"logs",
	"writeups",
	"data",
	"awd",
	"awdp",
] as const;

type TabKey = (typeof TAB_KEYS)[number];

function normalizeTab(raw: string | null): TabKey {
	return TAB_KEYS.find((key) => key === raw) ?? "config";
}

export function EventConsolePage(): ReactNode {
	const params = useParams();
	const eventId = params.eventId ?? "";
	const [searchParams, setSearchParams] = useSearchParams();
	const client = useClient();
	const queryClient = useQueryClient();

	const query = useQuery({
		queryKey: qk.admin.event(eventId),
		queryFn: () => call<Events>(client.admin.events.get(eventId), "赛事详情"),
		enabled: eventId !== "",
	});

	const event = query.data ?? null;
	useDocumentTitle(event ? `${event.title} · 赛事控制台 · FloatCTF` : "赛事控制台 · FloatCTF");

	// 家族专属标签：其它家族下如果 URL 带着这些 tab，回退到「配置」。
	const rawTab = normalizeTab(searchParams.get("tab"));
	const tab: TabKey =
		(rawTab === "awd" && event !== null && event.family !== "awd") ||
		(rawTab === "awdp" && event !== null && event.family !== "awdp")
			? "config"
			: rawTab;

	function changeTab(next: string) {
		const nextParams = new URLSearchParams(searchParams);
		nextParams.set("tab", next);
		setSearchParams(nextParams, { replace: true });
	}

	function refreshAll() {
		const keys = [
			qk.admin.event(eventId),
			qk.admin.eventChallenges(eventId),
			qk.admin.eventUsers(eventId),
			qk.admin.eventTeams(eventId),
			qk.admin.eventAnnouncements(eventId),
			qk.admin.eventInstances(eventId),
			qk.admin.eventLogs(eventId),
			qk.admin.eventWriteups(eventId),
			qk.admin.eventData(eventId),
			qk.awd.adminStatus(eventId),
			qk.awd.adminScores(eventId),
			qk.awdp.adminConfig(eventId),
			qk.awdp.adminScores(eventId),
		];
		for (const key of keys) void queryClient.invalidateQueries({ queryKey: key });
	}

	if (eventId === "") {
		return (
			<PageBody>
				<NotFoundBlock
					title="缺少赛事 ID"
					description="地址里没有 eventId，请从赛事管理列表进入。"
					action={
						<Button asChild>
							<Link to="/admin/events">返回赛事管理</Link>
						</Button>
					}
				/>
			</PageBody>
		);
	}

	if (query.isPending) {
		return (
			<PageBody>
				<LoadingBlock label="加载赛事详情…" />
			</PageBody>
		);
	}

	if (query.isError) {
		if (isForbidden(query.error)) {
			return (
				<PageBody>
					<PermissionDeniedBlock description="当前管理员账号无权读取该赛事（后端返回 403）。" />
				</PageBody>
			);
		}
		if (isNotFound(query.error)) {
			return (
				<PageBody>
					<NotFoundBlock
						title="赛事不存在"
						description="该赛事可能已被删除，或 ID 不正确。"
						action={
							<Button asChild>
								<Link to="/admin/events">返回赛事管理</Link>
							</Button>
						}
					/>
				</PageBody>
			);
		}
		return (
			<PageBody>
				<ErrorBlock
					error={query.error}
					title="加载赛事失败"
					onRetry={() => void query.refetch()}
				/>
			</PageBody>
		);
	}

	if (event === null) {
		return (
			<PageBody>
				<NotFoundBlock
					title="赛事不存在"
					description="后端未返回该赛事的数据。"
					action={
						<Button asChild>
							<Link to="/admin/events">返回赛事管理</Link>
						</Button>
					}
				/>
			</PageBody>
		);
	}

	return (
		<PageBody>
			<PageHeader
				breadcrumbs={
					<Link to="/admin/events" className="underline-offset-4 hover:underline">
						赛事管理
					</Link>
				}
				title={event.title}
				badge={<EventStatusPill event={event} />}
				description={
					<span className="flex flex-wrap items-center gap-2">
						<EventFamilyPill family={event.family} />
						<ParticipantModePill mode={event.participant_mode} />
						<EventPurposePills event={event} />
						<span className="tnum text-xs">
							{formatRange(event.start_time, event.end_time ?? null)}
						</span>
					</span>
				}
				actions={
					<Toolbar>
						<CopyText value={event.id} label="赛事 ID" />
						<Button variant="outline" onClick={refreshAll} disabled={query.isFetching}>
							<RefreshCw className={query.isFetching ? "animate-spin" : undefined} /> 刷新
						</Button>
						{event.family === "awd" ? (
							<Button variant="outline" asChild>
								<Link to={`/admin/events/${event.id}/awd`}>
									<Swords /> AWD 运维
								</Link>
							</Button>
						) : null}
						{event.family === "awdp" ? (
							<Button variant="outline" asChild>
								<Link to={`/admin/events/${event.id}/awdp`}>
									<Terminal /> AWDP 工作台
								</Link>
							</Button>
						) : null}
					</Toolbar>
				}
			/>

			<Tabs value={tab} onValueChange={changeTab}>
				<div className="overflow-x-auto">
					<TabsList variant="line" className="w-max min-w-full justify-start">
						<TabsTrigger value="config">配置</TabsTrigger>
						<TabsTrigger value="challenges">题目</TabsTrigger>
						<TabsTrigger value="members">成员</TabsTrigger>
						<TabsTrigger value="teams">战队</TabsTrigger>
						<TabsTrigger value="announcements">公告</TabsTrigger>
						<TabsTrigger value="instances">实例</TabsTrigger>
						<TabsTrigger value="logs">日志</TabsTrigger>
						<TabsTrigger value="writeups">Writeup</TabsTrigger>
						<TabsTrigger value="data">数据大屏</TabsTrigger>
						{event.family === "awd" ? <TabsTrigger value="awd">AWD 概览</TabsTrigger> : null}
						{event.family === "awdp" ? <TabsTrigger value="awdp">AWDP 概览</TabsTrigger> : null}
					</TabsList>
				</div>

				<TabsContent value="config" className="mt-4">
					<EventConfigTab event={event} />
				</TabsContent>
				<TabsContent value="challenges" className="mt-4">
					<EventChallengesTab eventId={event.id} />
				</TabsContent>
				<TabsContent value="members" className="mt-4">
					<EventMembersTab eventId={event.id} participantMode={event.participant_mode} />
				</TabsContent>
				<TabsContent value="teams" className="mt-4">
					<EventTeamsTab eventId={event.id} participantMode={event.participant_mode} />
				</TabsContent>
				<TabsContent value="announcements" className="mt-4">
					<EventAnnouncementsTab eventId={event.id} />
				</TabsContent>
				<TabsContent value="instances" className="mt-4">
					<EventInstancesTab eventId={event.id} />
				</TabsContent>
				<TabsContent value="logs" className="mt-4">
					<EventLogsTab eventId={event.id} />
				</TabsContent>
				<TabsContent value="writeups" className="mt-4">
					<EventWriteupsTab eventId={event.id} />
				</TabsContent>
				<TabsContent value="data" className="mt-4">
					<EventDataTab eventId={event.id} />
				</TabsContent>
				{event.family === "awd" ? (
					<TabsContent value="awd" className="mt-4">
						<EventAwdTab eventId={event.id} />
					</TabsContent>
				) : null}
				{event.family === "awdp" ? (
					<TabsContent value="awdp" className="mt-4">
						<EventAwdpTab eventId={event.id} />
					</TabsContent>
				) : null}
			</Tabs>
		</PageBody>
	);
}
