/**
 * 赛事控制台 · 配置标签。
 *
 * 可写字段以 SDK / 后端 `PatchEventRequest` 为准（title / description / hidden /
 * allow_join / rules / flag_prefix / start_time / end_time）；**家族、参赛模式、用途**
 * 后端 PATCH 请求体不接收（创建即固定），这里只读展示并说明原因，不做假交互。
 */

import { useState, type ReactNode } from "react";
import { PencilLine } from "lucide-react";

import { BooleanPill, TonePill } from "~/components/app/badges";
import { MarkdownView } from "~/components/app/markdown";
import { CopyText, KeyValueList, MonoText, SectionCard } from "~/components/app/page";
import { Button } from "~/components/ui/button";
import { EventPurpose } from "@floatctf/sdk/entity";
import type { Events } from "@floatctf/sdk/entity";
import { formatDateTime, formatRange } from "~/lib/format";
import {
	EVENT_FAMILY_LABEL,
	EVENT_PURPOSE_LABEL,
	PARTICIPANT_MODE_LABEL,
	computeEventStatus,
	EVENT_STATUS_LABEL,
	EVENT_STATUS_TONE,
} from "~/lib/event-status";

import { EventFormSheet } from "./event-form";

export function EventConfigTab({ event }: { event: Events }): ReactNode {
	const [editing, setEditing] = useState(false);
	const status = computeEventStatus(event.start_time, event.end_time ?? null);

	return (
		<div className="space-y-5">
			<SectionCard title="赛事身份" description="创建后固定的字段（后端 PATCH 请求体不接收）。">
				<KeyValueList
					columns={2}
					items={[
						{ key: "赛事 ID", value: <CopyText value={event.id} label="赛事 ID" /> },
						{
							key: "家族（family）",
							value: (
								<TonePill tone="neutral">{EVENT_FAMILY_LABEL[event.family] ?? event.family}</TonePill>
							),
							hint: "创建后不可变更",
						},
						{
							key: "参赛模式（participant_mode）",
							value: (
								<TonePill tone="info">
									{PARTICIPANT_MODE_LABEL[event.participant_mode] ?? event.participant_mode}
								</TonePill>
							),
							hint: "创建后不可变更",
						},
						{
							key: "用途（purpose）",
							value: (
								<TonePill tone={event.purpose === EventPurpose.Practice ? "warning" : "muted"}>
									{EVENT_PURPOSE_LABEL[event.purpose] ?? event.purpose}
								</TonePill>
							),
						},
						{
							key: "虚拟赛事（is_virtual）",
							value: <BooleanPill value={event.is_virtual} trueText="虚拟" falseText="正式" />,
							hint: "由 purpose=practice 派生",
						},
						{
							key: "系统托管",
							value: event.system_key ? (
								<MonoText>{event.system_key}</MonoText>
							) : (
								<span className="text-xs text-muted-foreground">否</span>
							),
							hint: event.system_key ? "系统托管赛事不能被普通接口修改 / 删除" : undefined,
						},
						{ key: "创建时间", value: <MonoText>{formatDateTime(event.created_at, { seconds: true })}</MonoText> },
						{
							key: "更新时间（updated_at）",
							value: <MonoText>{formatDateTime(event.updated_at, { seconds: true })}</MonoText>,
							hint: "保存配置时随 PATCH 一起回传（乐观并发）",
						},
					]}
				/>
			</SectionCard>

			<SectionCard
				title="可编辑配置"
				description="保存后立即对选手端生效；时间变更会同步 AWD / AWDP 的排期校验。"
				actions={
					<Button variant="outline" size="sm" onClick={() => setEditing(true)}>
						<PencilLine /> 编辑配置
					</Button>
				}
			>
				<KeyValueList
					columns={2}
					items={[
						{ key: "标题", value: event.title },
						{
							key: "状态",
							value: (
								<TonePill tone={EVENT_STATUS_TONE[status]}>{EVENT_STATUS_LABEL[status]}</TonePill>
							),
							hint: "由 start_time / end_time 推导",
						},
						{
							key: "时间窗",
							value: (
								<MonoText className="text-sm">
									{formatRange(event.start_time, event.end_time ?? null)}
								</MonoText>
							),
						},
						{
							key: "Flag 前缀",
							value: event.flag_prefix ? (
								<MonoText>{event.flag_prefix}</MonoText>
							) : (
								<span className="text-xs text-muted-foreground">未设置（后端使用默认）</span>
							),
						},
						{
							key: "隐藏赛事（hidden）",
							value: (
								<BooleanPill
									value={event.hidden}
									trueText="已隐藏"
									falseText="公开"
									trueTone="muted"
									falseTone="success"
								/>
							),
						},
						{
							key: "允许加入（allow_join）",
							value: (
								<BooleanPill
									value={event.allow_join}
									trueText="允许报名"
									falseText="不可报名"
									trueTone="success"
									falseTone="muted"
								/>
							),
						},
					]}
				/>
				<div className="mt-5 space-y-5">
					<div>
						<p className="mb-2 text-xs text-muted-foreground">赛事描述</p>
						<MarkdownView emptyText="未填写赛事描述。">{event.description}</MarkdownView>
					</div>
					<div>
						<p className="mb-2 text-xs text-muted-foreground">赛事规则</p>
						<MarkdownView emptyText="未填写赛事规则。">{event.rules}</MarkdownView>
					</div>
				</div>
			</SectionCard>

			{editing ? (
				<EventFormSheet
					open
					onOpenChange={(next) => {
						if (!next) setEditing(false);
					}}
					event={event}
				/>
			) : null}
		</div>
	);
}
