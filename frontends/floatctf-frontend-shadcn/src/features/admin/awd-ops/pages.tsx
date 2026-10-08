/**
 * 管理端 AWD 赛事运维页（`/admin/events/:eventId/awd`）。
 *
 * 定位（FRONTEND-PLAN §4）：赛事控制台只放只读摘要与跳转，**完整运维流程在本页**：
 * 赛事配置 → 分配赛事网络 → 部署 → 预检 → 开赛 → 暂停/恢复 → 结束 → 归档，
 * 外加令牌轮换、分数调整、封禁、GameBox 挂载与实例重置。
 *
 * 状态机驱动：所有按钮的可用性只由后端字段（status / phase / final_settlement /
 * 赛事网络是否已分配）决定，禁用时把原因写在按钮 title 与状态条里。
 */

import { useCallback } from "react";
import { Link, useParams } from "react-router";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, Boxes, RefreshCw } from "lucide-react";

import type { AwdEventStatus } from "@floatctf/sdk";

import { useBindings, useClient } from "~/api/client";
import { errorText } from "~/api/errors";
import { qk } from "~/api/keys";
import { RealtimePill } from "~/components/app/badges";
import { useConfirm } from "~/components/app/confirm";
import {
	KeyValueList,
	MonoText,
	PageBody,
	PageHeader,
	SectionCard,
} from "~/components/app/page";
import { EmptyBlock, ErrorBlock, LoadingBlock } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "~/components/ui/tabs";
import { formatDateTime } from "~/lib/format";
import { EVENT_FAMILY_LABEL, EVENT_PURPOSE_LABEL } from "~/lib/event-status";
import { useDocumentTitle } from "~/lib/hooks";

import { AwdConfigTab } from "./config-tab";
import { AwdGameboxesTab } from "./gameboxes-tab";
import { AwdInstancesTab } from "./instances-tab";
import { AwdNetworkTab } from "./network-tab";
import { AwdPrecheckTab } from "./precheck-tab";
import { useAwdEvent, useAwdEventNetwork, useAwdStatus } from "./queries";
import { AwdScoresTab } from "./scores-tab";
import {
	AWD_ACTION_CONFIRM,
	AWD_ACTION_LABEL,
	AwdStatusPill,
	awdStatusLabel,
	buildAwdGates,
	runAwdAction,
	type AwdAction,
} from "./shared";
import { AwdTeamsTab } from "./teams-tab";

const ACTION_ORDER: AwdAction[] = [
	"deploy",
	"precheck",
	"start",
	"pause",
	"resume",
	"finish",
	"archive",
	"rotateTokens",
];

const DANGER_ACTIONS: AwdAction[] = ["deploy", "start", "finish", "archive", "rotateTokens"];

export function AwdOpsPage() {
	useDocumentTitle("AWD 运维 · FloatCTF");
	const { eventId = "" } = useParams<{ eventId: string }>();
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();
	const { useAdminAwdEventStream } = useBindings();
	const stream = useAdminAwdEventStream({ eventId, enabled: eventId !== "" });

	const eventQuery = useAwdEvent(eventId);
	const statusQuery = useAwdStatus(eventId);
	const networkQuery = useAwdEventNetwork(eventId);

	const status: AwdEventStatus | null = statusQuery.data ?? null;
	const event = eventQuery.data;
	const networkAllocated = networkQuery.data !== null && networkQuery.data !== undefined;

	/** 手动刷新：既失效 SSE 覆盖的键，也覆盖 SSE 未包含的赛事 GameBox / 赛事网络 / 赛事本体。 */
	const refreshAll = useCallback(() => {
		void queryClient.invalidateQueries({ queryKey: qk.awd.adminStatus(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.awd.adminScores(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.awd.prechecks(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.awd.eventGameboxes(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.awd.eventNetwork(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.admin.event(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.admin.eventInstances(eventId) });
		stream.invalidateAwd();
	}, [queryClient, eventId, stream]);

	const lifecycle = useMutation({
		mutationFn: (action: AwdAction) => runAwdAction(client, eventId, action),
		onSuccess: (_result, action) => {
			toast.success(`${AWD_ACTION_LABEL[action]}已提交`, "赛事状态已更新。");
			refreshAll();
		},
		onError: (error, action) => toast.apiError(`${AWD_ACTION_LABEL[action]}失败`, error),
	});

	const gates = buildAwdGates({
		status: status?.status ?? null,
		networkAllocated,
		finalSettlement: status?.final_settlement ?? false,
	});

	if (eventQuery.isPending) {
		return (
			<PageBody>
				<PageHeader title="AWD 赛事运维" description="加载赛事信息…" />
				<LoadingBlock label="加载赛事…" />
			</PageBody>
		);
	}

	if (eventQuery.isError) {
		return (
			<PageBody>
				<PageHeader title="AWD 赛事运维" />
				<ErrorBlock error={eventQuery.error} title="加载赛事失败" onRetry={() => eventQuery.refetch()} />
				<Button variant="outline" size="sm" asChild>
					<Link to="/admin/events">
						<ArrowLeft /> 返回赛事列表
					</Link>
				</Button>
			</PageBody>
		);
	}

	return (
		<PageBody>
			<PageHeader
				title={`${event?.title ?? "赛事"} · AWD 运维`}
				description="AWD 全流程运维：配置、网络、部署、预检、生命周期、判罚与实例。"
				breadcrumbs={
					<span className="inline-flex items-center gap-1.5">
						<Link className="hover:text-foreground" to="/admin/events">
							赛事
						</Link>
						<span>/</span>
						<Link className="hover:text-foreground" to={`/admin/events/${eventId}`}>
							{event?.title ?? eventId}
						</Link>
						<span>/</span>
						<span>AWD 运维</span>
					</span>
				}
				badge={
					<AwdStatusPill
						status={status?.status ?? null}
						phase={status?.phase ?? null}
						finalSettlement={status?.final_settlement ?? false}
					/>
				}
				actions={
					<>
						<RealtimePill state={stream.connectionState} onRefresh={refreshAll} />
						<Button variant="outline" size="sm" asChild title="GameBox 库">
							<Link to="/admin/gameboxes">
								<Boxes /> GameBox 库
							</Link>
						</Button>
						<Button variant="outline" size="sm" asChild>
							<Link to={`/admin/events/${eventId}`}>
								<ArrowLeft /> 赛事控制台
							</Link>
						</Button>
					</>
				}
			/>

			<SectionCard
				title="生命周期操作"
				description="按钮可用性由后端状态决定；禁用原因写在按钮悬浮提示与下方说明里。"
				actions={
					<Button
						variant="ghost"
						size="sm"
						disabled={statusQuery.isFetching}
						onClick={() => void statusQuery.refetch()}
					>
						<RefreshCw /> 刷新状态
					</Button>
				}
			>
				<div className="space-y-3">
					<div className="flex flex-wrap items-center gap-2">
						{ACTION_ORDER.map((action) => {
							const gate = gates[action];
							return (
								<Button
									key={action}
									variant={DANGER_ACTIONS.includes(action) ? "destructive" : "outline"}
									size="sm"
									disabled={!gate.enabled || lifecycle.isPending}
									title={gate.reason ?? AWD_ACTION_LABEL[action]}
									onClick={async () => {
										const ok = await confirm(AWD_ACTION_CONFIRM[action]);
										if (ok) lifecycle.mutate(action);
									}}
								>
									{AWD_ACTION_LABEL[action]}
								</Button>
							);
						})}
					</div>

					{ACTION_ORDER.filter((action) => !gates[action].enabled).length > 0 ? (
						<details className="text-xs text-muted-foreground">
							<summary className="cursor-pointer">
								当前不可用的操作（
								{ACTION_ORDER.filter((action) => !gates[action].enabled).length}）
							</summary>
							<ul className="mt-2 space-y-1">
								{ACTION_ORDER.filter((action) => !gates[action].enabled).map((action) => (
									<li key={action}>
										<span className="text-foreground">{AWD_ACTION_LABEL[action]}</span>
										<span> · {gates[action].reason}</span>
									</li>
								))}
							</ul>
						</details>
					) : null}

					{status === null ? (
						<EmptyBlock
							title="尚未开通 AWD"
							description="在「配置」标签页开通 AWD 后，这里会出现部署 / 预检 / 开赛等操作。"
						/>
					) : (
						<KeyValueList
							columns={3}
							items={[
								{ key: "赛事状态", value: awdStatusLabel(status.status) },
								{ key: "当前阶段", value: status.phase },
								{
									key: "总轮次",
									value: status.round_count === null ? "未配置" : status.round_count,
								},
								{ key: "单轮时长", value: `${status.round_duration_secs} 秒` },
								{ key: "初始分数", value: status.initial_score },
								{
									key: "免费重置 / 额外罚分",
									value: `${status.free_reset_count} 次 / ${status.extra_reset_penalty} 分`,
								},
								{
									key: "计划开赛",
									value: status.planned_start_at
										? formatDateTime(status.planned_start_at, { seconds: true })
										: "手动开赛",
								},
								{
									key: "预检通过",
									value: status.verified_at
										? formatDateTime(status.verified_at, { seconds: true })
										: "未通过",
								},
								{
									key: "实际开赛",
									value: status.started_at
										? formatDateTime(status.started_at, { seconds: true })
										: "未开赛",
								},
								{
									key: "终局结算",
									value: status.final_settlement ? "进行中（竞赛操作已关闭）" : "未进入",
								},
								{
									key: "赛事网络",
									value: networkAllocated ? (
										"已分配"
									) : (
										<span className="text-destructive">未分配</span>
									),
								},
								{
									key: "配置版本（乐观锁）",
									value: <MonoText>{status.updated_at}</MonoText>,
								},
							]}
						/>
					)}

					{networkQuery.isError ? (
						<p className="text-xs text-destructive">
							赛事网络状态读取失败（不是「未分配」）：{errorText(networkQuery.error)}。部署与预检的可用性判断可能不准确。
						</p>
					) : null}

					{event ? (
						<p className="text-xs text-muted-foreground">
							赛制：{EVENT_FAMILY_LABEL[event.family] ?? event.family} · 用途：
							{EVENT_PURPOSE_LABEL[event.purpose] ?? event.purpose} · 时间窗：
							{formatDateTime(event.start_time, { seconds: true })} –{" "}
							{formatDateTime(event.end_time, { seconds: true })}
						</p>
					) : null}
				</div>
			</SectionCard>

			<Tabs defaultValue="config">
				<TabsList className="no-scrollbar flex w-full justify-start overflow-x-auto">
					<TabsTrigger value="config">配置</TabsTrigger>
					<TabsTrigger value="precheck">预检与轮换</TabsTrigger>
					<TabsTrigger value="scores">积分与判罚</TabsTrigger>
					<TabsTrigger value="teams">战队</TabsTrigger>
					<TabsTrigger value="gameboxes">GameBox 挂载</TabsTrigger>
					<TabsTrigger value="network">网络</TabsTrigger>
					<TabsTrigger value="instances">实例</TabsTrigger>
				</TabsList>
				<TabsContent value="config" className="mt-4">
					<AwdConfigTab eventId={eventId} />
				</TabsContent>
				<TabsContent value="precheck" className="mt-4">
					<AwdPrecheckTab eventId={eventId} />
				</TabsContent>
				<TabsContent value="scores" className="mt-4">
					<AwdScoresTab eventId={eventId} />
				</TabsContent>
				<TabsContent value="teams" className="mt-4">
					<AwdTeamsTab eventId={eventId} />
				</TabsContent>
				<TabsContent value="gameboxes" className="mt-4">
					<AwdGameboxesTab eventId={eventId} />
				</TabsContent>
				<TabsContent value="network" className="mt-4">
					<AwdNetworkTab eventId={eventId} />
				</TabsContent>
				<TabsContent value="instances" className="mt-4">
					<AwdInstancesTab eventId={eventId} />
				</TabsContent>
			</Tabs>
		</PageBody>
	);
}
