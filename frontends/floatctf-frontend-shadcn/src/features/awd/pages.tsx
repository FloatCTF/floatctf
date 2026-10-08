/**
 * `/arena/:eventId` —— AWD 驾驶舱（单一工作区，不是 Default 的多页）。
 *
 * 左列：攻击面板（flag 提交 + 我的 GameBox 表 + 行内重置）
 * 右列：轮次/状态卡 + 实时积分榜（我的队伍高亮）
 * 底部：网络凭据（WireGuard / SSH）
 *
 * 所有数据来自 `client.awd.player.*` 与 `client.service.events.get`；
 * 实时通道为 `useAwdEventStream`（`RealtimePill` 显式呈现连接态，含 `auth_error`）。
 */

import type { ReactNode } from "react";
import { Link, useParams } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, CalendarDays, Flag, Swords, Trophy } from "lucide-react";

import type { EventInfo, AwdPlayerStatus } from "@floatctf/sdk";

import { call } from "~/api/call";
import { isForbidden, isNotFound } from "~/api/errors";
import { useBindings, useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { RealtimePill, TonePill } from "~/components/app/badges";
import { KeyValueList, MonoText, PageBody, PageHeader, SectionCard, StatCard } from "~/components/app/page";
import {
	EmptyBlock,
	ErrorBlock,
	LoadingBlock,
	PermissionDeniedBlock,
} from "~/components/app/states";
import { Button } from "~/components/ui/button";
import { formatInt, formatScore } from "~/lib/format";
import { useDocumentTitle } from "~/lib/hooks";

import {
	AwdFlagPanel,
	AwdGameboxPanel,
	AwdNetworkPanel,
	AwdScoreboardPanel,
	AwdStatePills,
} from "./components";
import { awdPhaseLabel, awdStatusLabel } from "./domain";

export function ArenaPage(): ReactNode {
	const { eventId = "" } = useParams<{ eventId: string }>();
	useDocumentTitle("AWD 驾驶舱 · FloatCTF");

	const client = useClient();
	const { useAwdEventStream } = useBindings();
	const stream = useAwdEventStream({ eventId, enabled: Boolean(eventId) });

	const statusQuery = useQuery({
		queryKey: qk.awd.status(eventId),
		queryFn: () =>
			call<AwdPlayerStatus>(client.awd.player.status(eventId), "AWD 赛事状态"),
		enabled: Boolean(eventId),
		retry: false,
	});

	const eventQuery = useQuery({
		queryKey: qk.events.detail(eventId),
		queryFn: () => call<EventInfo>(client.service.events.get(eventId), "赛事详情"),
		enabled: Boolean(eventId),
	});

	const status = statusQuery.data;
	const eventInfo = eventQuery.data;
	const myTeamId = eventInfo?.team_result?.team.id ?? null;
	const eventTitle = eventInfo?.event.title ?? "AWD 驾驶舱";

	return (
		<PageBody>
			<PageHeader
				title={eventTitle}
				description="AWD 竞赛驾驶舱：提交 flag 攻击对手、监控并重置自己的 GameBox、下发网络凭据。"
				breadcrumbs={
					<nav className="flex items-center gap-1.5">
						<Link to="/events" className="hover:underline">
							赛事
						</Link>
						<span>/</span>
						<Link to={`/events/${eventId}`} className="hover:underline">
							赛事工作区
						</Link>
						<span>/</span>
						<span className="text-foreground">AWD 驾驶舱</span>
					</nav>
				}
				badge={
					status ? (
						<AwdStatePills
							status={status.status}
							phase={status.phase}
							banned={status.banned}
							finalSettlement={status.final_settlement}
						/>
					) : null
				}
				actions={
					<RealtimePill
						state={stream.connectionState}
						onRefresh={() => stream.invalidateAwd()}
					/>
				}
			/>

			{statusQuery.isPending ? (
				<LoadingBlock label="加载 AWD 赛事状态…" />
			) : statusQuery.isError ? (
				isNotFound(statusQuery.error) ? (
					// 赛事没有 AWD 配置时后端返回 404（「未找到该 AWD 赛事」）——这是业务态，不是加载失败。
					<EmptyBlock
						title="该赛事未启用 AWD"
						description="本赛事没有 AWD 赛制配置；请从赛事列表确认赛制，或联系管理员。"
						action={
							<Button variant="outline" size="sm" asChild>
								<Link to="/events">返回赛事列表</Link>
							</Button>
						}
					/>
				) : isForbidden(statusQuery.error) ? (
					<PermissionDeniedBlock description="你不是本赛事的参赛成员（或已离队），无法查看 AWD 驾驶舱。" />
				) : (
					<ErrorBlock
						error={statusQuery.error}
						title="加载 AWD 状态失败"
						onRetry={() => statusQuery.refetch()}
					/>
				)
			) : status ? (
				<div className="space-y-5">
						{status.banned ? (
							<div
								className="flex items-start gap-2 rounded-lg border border-destructive/40 bg-destructive/5 px-4 py-3 text-sm"
								role="alert"
							>
								<AlertTriangle className="mt-0.5 size-4 shrink-0 text-destructive" />
								<div>
									<p className="font-medium text-destructive">你的队伍已被禁赛</p>
									<p className="text-muted-foreground">
										禁赛期间 flag 提交、GameBox 重置与网络访问均被后端拒绝；
										如有疑问请联系赛事管理员。
									</p>
								</div>
							</div>
						) : null}

						{status.final_settlement ? (
							<div
								className="flex items-start gap-2 rounded-lg border border-[var(--warning)]/40 bg-[var(--warning)]/10 px-4 py-3 text-sm"
								role="status"
							>
								<AlertTriangle className="mt-0.5 size-4 shrink-0 text-[var(--warning)]" />
								<div>
									<p className="font-medium">最终结算中</p>
									<p className="text-muted-foreground">
										最后一轮已结束，Judge 仍在结算；比赛已关闭，不再接受新的提交。
									</p>
								</div>
							</div>
						) : null}

						{eventQuery.isError ? (
							<ErrorBlock
								error={eventQuery.error}
								title="加载赛事信息失败"
								onRetry={() => eventQuery.refetch()}
							/>
						) : null}

						<div className="grid gap-5 lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
							<div className="space-y-5">
								<AwdFlagPanel eventId={eventId} status={status} />
								<AwdGameboxPanel eventId={eventId} status={status} />
							</div>

							<div className="space-y-5">
								<SectionCard
									title="轮次与状态"
									description="阶段与轮次由后端推进，本页只展示真实值。"
								>
									<div className="grid gap-3 sm:grid-cols-2">
										<StatCard
											label="我的分数"
											value={
												status.score === null ? "—" : formatScore(status.score)
											}
											hint="攻击分 + 防守分"
											icon={<Trophy className="size-5" />}
											tone="success"
										/>
										<StatCard
											label="当前轮次"
											value={
												status.current_round === null
													? "—"
													: formatInt(status.current_round)
											}
											hint={
												status.round_count === null
													? "总轮次未配置"
													: `共 ${status.round_count} 轮`
											}
											icon={<Flag className="size-5" />}
										/>
									</div>
									<KeyValueList
										className="mt-4"
										columns={1}
										items={[
											{
												key: "赛事状态",
												value: (
													<span className="flex items-center gap-2">
														<span>{awdStatusLabel(status.status)}</span>
														<MonoText className="text-muted-foreground">
															{status.status}
														</MonoText>
													</span>
												),
											},
											{
												key: "阶段",
												value: (
													<span className="flex items-center gap-2">
														<span>{awdPhaseLabel(status.phase)}</span>
														<MonoText className="text-muted-foreground">
															{status.phase}
														</MonoText>
													</span>
												),
											},
											{
												key: "队伍状态",
												value: status.banned ? (
													<TonePill tone="danger">已禁赛</TonePill>
												) : (
													<TonePill tone="success">正常</TonePill>
												),
											},
											{
												key: "最终结算",
												value: status.final_settlement ? (
													<TonePill tone="warning">进行中</TonePill>
												) : (
													<TonePill tone="muted">否</TonePill>
												),
											},
										]}
									/>
									{eventInfo ? (
										<div className="mt-4 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
											<CalendarDays className="size-3.5" />
											<span>{eventInfo.event.family.toUpperCase()}</span>
											<span>·</span>
											<span>
												{eventInfo.joined ? "已加入赛事" : "尚未加入赛事"}
											</span>
										</div>
									) : null}
								</SectionCard>

								<AwdScoreboardPanel eventId={eventId} myTeamId={myTeamId} />
							</div>
						</div>

						<AwdNetworkPanel eventId={eventId} />

						<SectionCard
							title="攻击流程提示"
							description="能力语义与 Default 一致，操作路径由本前端重新设计。"
						>
							<ul className="list-inside list-disc space-y-1 text-sm text-muted-foreground">
								<li>
									先用 WireGuard 接入赛事内网，再用 SSH 登录自己的 GameBox（见上方网络凭据）。
								</li>
								<li>
									在<strong className="text-foreground">加固期</strong>
									只允许修复自身漏洞；进入<strong className="text-foreground">攻击期</strong>
									后才能提交 flag。
								</li>
								<li>
									重置 GameBox 会销毁并按原始镜像重建容器，受免费次数与罚分约束，请谨慎使用。
								</li>
							</ul>
							<div className="mt-3 flex flex-wrap gap-2">
								<Button variant="outline" size="sm" asChild>
									<Link to={`/events/${eventId}`}>
										<Swords /> 返回赛事工作区
									</Link>
								</Button>
							</div>
						</SectionCard>
				</div>
			) : null}
		</PageBody>
	);
}
