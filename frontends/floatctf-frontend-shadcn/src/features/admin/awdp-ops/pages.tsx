/**
 * 管理端 AWDP 赛事运维页（`/admin/events/:eventId/awdp`）。
 *
 * 阶段是**线性**状态机（pending → break → preparing_fix → fix → ended）：
 * 后端 tick 会在时间点自动推进（Break 到期 → 准备修复 → 修复；最后一轮 cutoff → 结束），
 * 页面上的按钮只用于**提前**推进，并按后端返回的 phase 决定可用性。
 *
 * 实时性说明：后端没有管理端 AWDP SSE 通道（只有选手端 `/api/events/{id}/awdp/stream`），
 * 因此本页用 30s 轮询 + 显式刷新，而不是挂一个连不上的管理端流。
 */

import { useCallback } from "react";
import { Link, useParams } from "react-router";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, RefreshCw } from "lucide-react";

import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { useConfirm } from "~/components/app/confirm";
import { KeyValueList, PageBody, PageHeader, SectionCard } from "~/components/app/page";
import { ErrorBlock, LoadingBlock } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "~/components/ui/tabs";
import { formatDuration } from "~/lib/format";
import { EVENT_FAMILY_LABEL } from "~/lib/event-status";
import { useDocumentTitle } from "~/lib/hooks";

import { AwdpConfigTab } from "./config-tab";
import { AwdpDataTab } from "./data-tab";
import { AwdpGameboxesTab } from "./gameboxes-tab";
import { AwdpInstancesTab } from "./instances-tab";
import { useAwdpConfig, useAwdpEvent } from "./queries";
import { AwdpScoresTab } from "./scores-tab";
import {
	AWDP_ACTION_CONFIRM,
	AWDP_ACTION_LABEL,
	AWDP_PHASE_DESCRIPTION,
	AwdpPhasePill,
	buildAwdpGates,
	runAwdpAction,
	type AwdpAction,
} from "./shared";

const ACTION_ORDER: AwdpAction[] = ["start", "breakToFix", "finish"];

export function AwdpOpsPage() {
	useDocumentTitle("AWDP 运维 · FloatCTF");
	const { eventId = "" } = useParams<{ eventId: string }>();
	const client = useClient();
	const queryClient = useQueryClient();
	const confirm = useConfirm();

	const eventQuery = useAwdpEvent(eventId);
	const configQuery = useAwdpConfig(eventId);

	const phase = configQuery.data?.phase ?? null;

	const refreshAll = useCallback(() => {
		void queryClient.invalidateQueries({ queryKey: qk.awdp.adminConfig(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.awdp.adminGameboxes(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.awdp.adminInstances(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.awdp.adminScores(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.awdp.adminData(eventId) });
		void queryClient.invalidateQueries({ queryKey: qk.admin.event(eventId) });
	}, [queryClient, eventId]);

	const lifecycle = useMutation({
		mutationFn: (action: AwdpAction) => runAwdpAction(client, eventId, action),
		onSuccess: (_result, action) => {
			toast.success(`${AWDP_ACTION_LABEL[action]}已提交`, "阶段已更新。");
			refreshAll();
		},
		onError: (error, action) => toast.apiError(`${AWDP_ACTION_LABEL[action]}失败`, error),
	});

	const gates = buildAwdpGates(phase);
	const event = eventQuery.data;

	if (eventQuery.isPending) {
		return (
			<PageBody>
				<PageHeader title="AWDP 赛事运维" description="加载赛事信息…" />
				<LoadingBlock label="加载赛事…" />
			</PageBody>
		);
	}

	if (eventQuery.isError) {
		return (
			<PageBody>
				<PageHeader title="AWDP 赛事运维" />
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
				title={`${event?.title ?? "赛事"} · AWDP 运维`}
				description="AWDP 全流程运维：配置、阶段推进、GameBox 挂载、实例与积分。"
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
						<span>AWDP 运维</span>
					</span>
				}
				badge={<AwdpPhasePill phase={phase} />}
				actions={
					<>
						<Button
							variant="outline"
							size="sm"
							disabled={configQuery.isFetching}
							onClick={() => {
								void configQuery.refetch();
								refreshAll();
							}}
						>
							<RefreshCw /> 刷新
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
				title="阶段推进"
				description="后端 tick 会按时间自动推进；这里的按钮用于提前推进，可用性由当前阶段决定。"
			>
				<div className="space-y-3">
					<div className="flex flex-wrap items-center gap-2">
						<AwdpPhasePill phase={phase} />
						<span className="text-xs text-muted-foreground">
							{AWDP_PHASE_DESCRIPTION[phase ?? ""] ?? "尚未读取到阶段。"}
						</span>
					</div>

					<div className="flex flex-wrap items-center gap-2">
						{ACTION_ORDER.map((action) => {
							const gate = gates[action];
							return (
								<Button
									key={action}
									variant={action === "breakToFix" ? "outline" : "destructive"}
									size="sm"
									disabled={!gate.enabled || lifecycle.isPending}
									title={gate.reason ?? AWDP_ACTION_LABEL[action]}
									onClick={async () => {
										const ok = await confirm(AWDP_ACTION_CONFIRM[action]);
										if (ok) lifecycle.mutate(action);
									}}
								>
									{AWDP_ACTION_LABEL[action]}
								</Button>
							);
						})}
					</div>

					<ul className="space-y-1 text-xs text-muted-foreground">
						<li>Break 到期后由 tick 自动推进到修复阶段；Fix 最后一轮 cutoff 后自动结束。</li>
						<li>
							「攻破 → 修复」会把全部实例重置为 pristine（runtime_generation +1，公开端口不变）；
							重置失败会停在 preparing_fix，可用同一按钮重试。
						</li>
						{ACTION_ORDER.filter((action) => !gates[action].enabled).map((action) => (
							<li key={action}>
								<span className="text-foreground">{AWDP_ACTION_LABEL[action]} 暂不可用</span>
								<span> · {gates[action].reason}</span>
							</li>
						))}
					</ul>

					{configQuery.data ? (
						<KeyValueList
							columns={3}
							items={[
								{
									key: "总时长（Break + Fix）",
									value: formatDuration(
										configQuery.data.break_duration_secs + configQuery.data.fix_duration_secs,
									),
								},
								{ key: "总回合数", value: configQuery.data.total_rounds },
								{ key: "当前回合", value: configQuery.data.current_round },
								{ key: "Break 分值", value: configQuery.data.break_score },
								{ key: "每回合 Fix 分值", value: configQuery.data.fix_round_score },
								{
									key: "配置代数",
									value: configQuery.data.configuration_generation,
								},
							]}
						/>
					) : null}

					{configQuery.isError ? (
						<ErrorBlock
							error={configQuery.error}
							title="加载 AWDP 配置失败（后端要求赛事赛制为 AWDP）"
							onRetry={() => configQuery.refetch()}
						/>
					) : null}
				</div>
			</SectionCard>

			<Tabs defaultValue="config">
				<TabsList className="no-scrollbar flex w-full justify-start overflow-x-auto">
					<TabsTrigger value="config">配置</TabsTrigger>
					<TabsTrigger value="gameboxes">GameBox 挂载</TabsTrigger>
					<TabsTrigger value="instances">实例</TabsTrigger>
					<TabsTrigger value="scores">积分榜</TabsTrigger>
					<TabsTrigger value="data">数据大屏</TabsTrigger>
				</TabsList>
				<TabsContent value="config" className="mt-4">
					<AwdpConfigTab eventId={eventId} />
				</TabsContent>
				<TabsContent value="gameboxes" className="mt-4">
					<AwdpGameboxesTab eventId={eventId} />
				</TabsContent>
				<TabsContent value="instances" className="mt-4">
					<AwdpInstancesTab eventId={eventId} />
				</TabsContent>
				<TabsContent value="scores" className="mt-4">
					<AwdpScoresTab eventId={eventId} />
				</TabsContent>
				<TabsContent value="data" className="mt-4">
					<AwdpDataTab eventId={eventId} />
				</TabsContent>
			</Tabs>

			{event ? (
				<p className="text-xs text-muted-foreground">
					赛制：{EVENT_FAMILY_LABEL[event.family] ?? event.family} · 参与模式：
					{event.participant_mode}
				</p>
			) : null}
		</PageBody>
	);
}
