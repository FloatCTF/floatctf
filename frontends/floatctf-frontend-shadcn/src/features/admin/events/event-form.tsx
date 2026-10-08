/**
 * 赛事新建 / 编辑表单（`<FormSheet>`）。
 *
 * 只发送后端 `CreateEventRequest` / `PatchEventRequest` 真正接收的字段：
 * - 创建：family / participant_mode / purpose(可省略=competition) / title / description /
 *   hidden / allow_join / rules / flag_prefix / start_time / end_time；
 * - 编辑：title / description / hidden / allow_join / rules / flag_prefix / start_time / end_time
 *   （**family / participant_mode / purpose 创建后不可变更** —— 后端 PATCH 请求体里没有这三个字段，
 *   发过去也会被忽略，因此这里只读展示，不做「看似能改」的假交互）。
 */

import { useId, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Wand2 } from "lucide-react";

import { call } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { Field, FormFooter, FormGrid, FormSheet } from "~/components/app/form";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "~/components/ui/select";
import { Switch } from "~/components/ui/switch";
import { Textarea } from "~/components/ui/textarea";
import { EventFamily, EventPurpose, ParticipantMode } from "@floatctf/sdk/entity";
import type { Events } from "@floatctf/sdk/entity";
import { fromDatetimeLocalValue, toDatetimeLocalValue } from "~/lib/format";
import { EVENT_FAMILY_LABEL, PARTICIPANT_MODE_LABEL } from "~/lib/event-status";

import { EventFamilyPill, ParticipantModePill } from "./shared";

/** 赛事规则模板（仅作为「填充模板」按钮的内容，不是数据）。 */
const RULES_TEMPLATE = `# 赛事规则

## 一、比赛时间
- 开始 / 结束时间以赛事详情页公布为准

## 二、参赛说明
- 请使用真实信息参赛，报名截止后成员与报名信息不可修改

## 三、计分规则
- 每道题按赛事分值计分，重复提交不重复计分
- 排行榜按总分排序，同分时按达到分数的先后排名

## 四、Flag 格式
- Flag 形如 \`flag{...}\`，提交时请勿携带多余空格或换行

## 五、公平竞赛
- 禁止攻击比赛平台、其他参赛者或比赛基础设施
- 禁止共享 Flag / 答案 / WriteUp，违者取消资格与成绩
`;

interface EventFormState {
	title: string;
	description: string;
	family: EventFamily;
	participant_mode: ParticipantMode;
	start_time: string;
	end_time: string;
	hidden: boolean;
	allow_join: boolean;
	flag_prefix: string;
	rules: string;
}

function initialState(event: Events | null): EventFormState {
	if (event) {
		return {
			title: event.title,
			description: event.description ?? "",
			family: event.family,
			participant_mode: event.participant_mode,
			start_time: toDatetimeLocalValue(event.start_time),
			end_time: toDatetimeLocalValue(event.end_time ?? null),
			hidden: event.hidden,
			allow_join: event.allow_join,
			flag_prefix: event.flag_prefix ?? "",
			rules: event.rules,
		};
	}
	return {
		title: "",
		description: "",
		family: EventFamily.Jeopardy,
		participant_mode: ParticipantMode.Individual,
		start_time: "",
		end_time: "",
		hidden: false,
		allow_join: true,
		flag_prefix: "flag",
		rules: "",
	};
}

export function EventFormSheet({
	open,
	onOpenChange,
	event,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	/** 有值 = 编辑；`null` = 新建。 */
	event: Events | null;
}) {
	const client = useClient();
	const queryClient = useQueryClient();
	const formId = useId();
	const [form, setForm] = useState<EventFormState>(() => initialState(event));
	const [errors, setErrors] = useState<{
		title?: string;
		start_time?: string;
		end_time?: string;
	}>({});

	const isEdit = event !== null;
	// 后端：competition 赛事必须有 end_time；practice（虚拟）赛事可以没有。
	const endRequired = event === null || event.purpose !== EventPurpose.Practice;

	const save = useMutation({
		mutationFn: (input: { id: string | null; payload: Partial<Events> }) =>
			input.id === null
				? call<Events>(client.admin.events.create(input.payload), "创建赛事")
				: call<Events>(client.admin.events.patch(input.payload), "保存赛事"),
		onSuccess: (saved, input) => {
			toast.success(input.id === null ? "赛事已创建" : "赛事已更新");
			// `qk.admin.events()` 的 `{}` 在 TanStack 的 partial match 中匹配全部参数组合。
			void queryClient.invalidateQueries({ queryKey: qk.admin.events() });
			void queryClient.invalidateQueries({ queryKey: qk.admin.event(saved.id) });
			onOpenChange(false);
		},
		onError: (error) => toast.apiError(isEdit ? "保存赛事失败" : "创建赛事失败", error),
	});

	function submit() {
		const nextErrors: typeof errors = {};
		if (form.title.trim() === "") nextErrors.title = "赛事标题不能为空";
		const startIso = fromDatetimeLocalValue(form.start_time);
		const endIso = fromDatetimeLocalValue(form.end_time);
		if (startIso === null) nextErrors.start_time = "请选择开始时间";
		if (endRequired && endIso === null) nextErrors.end_time = "正式赛事必须设置结束时间";
		if (startIso !== null && endIso !== null && startIso >= endIso) {
			nextErrors.end_time = "结束时间必须晚于开始时间";
		}
		setErrors(nextErrors);
		if (Object.keys(nextErrors).length > 0) return;

		const flagPrefix = form.flag_prefix.trim();
		if (event === null) {
			save.mutate({
				id: null,
				payload: {
					family: form.family,
					participant_mode: form.participant_mode,
					title: form.title.trim(),
					description: form.description,
					hidden: form.hidden,
					allow_join: form.allow_join,
					rules: form.rules,
					flag_prefix: flagPrefix === "" ? undefined : flagPrefix,
					start_time: startIso as string,
					end_time: endIso as string,
				},
			});
			return;
		}

		save.mutate({
			id: event.id,
			payload: {
				id: event.id,
				// 乐观并发：把读到的 `updated_at` 一起回传（后端 PatchEventRequest 目前不消费该字段，
				// 语义是「我基于这个版本修改」；见交付报告）。
				updated_at: event.updated_at,
				title: form.title.trim(),
				description: form.description,
				hidden: form.hidden,
				allow_join: form.allow_join,
				rules: form.rules,
				flag_prefix: flagPrefix,
				start_time: startIso as string,
				...(endIso !== null ? { end_time: endIso } : {}),
			},
		});
	}

	return (
		<FormSheet
			open={open}
			onOpenChange={onOpenChange}
			width="lg"
			title={isEdit ? "编辑赛事" : "新建赛事"}
			description={
				isEdit
					? "家族 / 参赛模式在创建后不可变更；其余字段保存后立即生效。"
					: "创建后家族与参赛模式不可变更，请先确认赛制。"
			}
			footer={
				<FormFooter
					formId={formId}
					onCancel={() => onOpenChange(false)}
					isPending={save.isPending}
					submitLabel={isEdit ? "保存" : "创建赛事"}
				/>
			}
		>
			<form
				id={formId}
				className="space-y-4"
				onSubmit={(submitEvent) => {
					submitEvent.preventDefault();
					submit();
				}}
			>
				<Field label="赛事标题" htmlFor={`${formId}-title`} required error={errors.title}>
					<Input
						id={`${formId}-title`}
						value={form.title}
						maxLength={200}
						onChange={(changeEvent) => setForm({ ...form, title: changeEvent.target.value })}
						placeholder="例如：2026 春季校内赛"
					/>
				</Field>

				<Field
					label="赛事描述"
					htmlFor={`${formId}-description`}
					hint="最多 10000 字符，支持 Markdown。"
				>
					<Textarea
						id={`${formId}-description`}
						value={form.description}
						rows={3}
						onChange={(changeEvent) => setForm({ ...form, description: changeEvent.target.value })}
					/>
				</Field>

				{isEdit ? (
					<div className="space-y-2 rounded-md border bg-muted/30 px-3 py-2">
						<p className="text-xs text-muted-foreground">
							家族与参赛模式创建后不可变更（后端 PATCH 请求体不接收这两个字段）。
						</p>
						<div className="flex flex-wrap items-center gap-2">
							<EventFamilyPill family={form.family} />
							<ParticipantModePill mode={form.participant_mode} />
							<span className="text-xs text-muted-foreground">
								{EVENT_FAMILY_LABEL[form.family] ?? form.family} ·{" "}
								{PARTICIPANT_MODE_LABEL[form.participant_mode] ?? form.participant_mode}
							</span>
						</div>
					</div>
				) : (
					<FormGrid>
						<Field label="家族（赛制）" required hint="AWD 固定为战队赛。">
							<Select
								value={form.family}
								onValueChange={(value) => {
									const family = value as EventFamily;
									setForm({
										...form,
										family,
										participant_mode:
											family === EventFamily.Awd ? ParticipantMode.Team : form.participant_mode,
									});
								}}
							>
								<SelectTrigger className="w-full">
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									<SelectItem value={EventFamily.Jeopardy}>
										{EVENT_FAMILY_LABEL[EventFamily.Jeopardy]}
									</SelectItem>
									<SelectItem value={EventFamily.Awd}>
										{EVENT_FAMILY_LABEL[EventFamily.Awd]}
									</SelectItem>
									<SelectItem value={EventFamily.Awdp}>
										{EVENT_FAMILY_LABEL[EventFamily.Awdp]}
									</SelectItem>
								</SelectContent>
							</Select>
						</Field>
						<Field label="参赛模式" required hint="创建后不可变更。">
							<Select
								value={form.participant_mode}
								disabled={form.family === EventFamily.Awd}
								onValueChange={(value) =>
									setForm({ ...form, participant_mode: value as ParticipantMode })
								}
							>
								<SelectTrigger className="w-full">
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									{form.family === EventFamily.Awd ? (
										<SelectItem value={ParticipantMode.Team}>
											{PARTICIPANT_MODE_LABEL[ParticipantMode.Team]}
										</SelectItem>
									) : (
										<>
											<SelectItem value={ParticipantMode.Individual}>
												{PARTICIPANT_MODE_LABEL[ParticipantMode.Individual]}
											</SelectItem>
											<SelectItem value={ParticipantMode.Team}>
												{PARTICIPANT_MODE_LABEL[ParticipantMode.Team]}
											</SelectItem>
										</>
									)}
								</SelectContent>
							</Select>
						</Field>
					</FormGrid>
				)}

				<FormGrid>
					<Field label="开始时间" htmlFor={`${formId}-start`} required error={errors.start_time}>
						<Input
							id={`${formId}-start`}
							type="datetime-local"
							value={form.start_time}
							onChange={(changeEvent) => setForm({ ...form, start_time: changeEvent.target.value })}
						/>
					</Field>
					<Field
						label="结束时间"
						htmlFor={`${formId}-end`}
						required={endRequired}
						hint={endRequired ? undefined : "练习（虚拟）赛事可以留空。"}
						error={errors.end_time}
					>
						<Input
							id={`${formId}-end`}
							type="datetime-local"
							value={form.end_time}
							onChange={(changeEvent) => setForm({ ...form, end_time: changeEvent.target.value })}
						/>
					</Field>
				</FormGrid>

				<FormGrid>
					<Field label="Flag 前缀" htmlFor={`${formId}-prefix`} hint="最多 32 字符，例如 flag。">
						<Input
							id={`${formId}-prefix`}
							value={form.flag_prefix}
							maxLength={32}
							onChange={(changeEvent) => setForm({ ...form, flag_prefix: changeEvent.target.value })}
						/>
					</Field>
					<div className="flex flex-col gap-3">
						<label className="flex items-center justify-between gap-3 text-sm">
							<span>
								隐藏赛事
								<span className="ml-2 text-xs text-muted-foreground">选手端列表不可见</span>
							</span>
							<Switch
								checked={form.hidden}
								onCheckedChange={(checked) => setForm({ ...form, hidden: checked })}
								aria-label="隐藏赛事"
							/>
						</label>
						<label className="flex items-center justify-between gap-3 text-sm">
							<span>
								允许加入
								<span className="ml-2 text-xs text-muted-foreground">选手可自行报名</span>
							</span>
							<Switch
								checked={form.allow_join}
								onCheckedChange={(checked) => setForm({ ...form, allow_join: checked })}
								aria-label="允许加入"
							/>
						</label>
					</div>
				</FormGrid>

				<Field
					label="赛事规则"
					htmlFor={`${formId}-rules`}
					hint="支持 Markdown，最多 50000 字符。"
					actions={
						<Button
							type="button"
							variant="ghost"
							size="xs"
							onClick={() => setForm({ ...form, rules: RULES_TEMPLATE })}
						>
							<Wand2 /> 填充模板
						</Button>
					}
				>
					<Textarea
						id={`${formId}-rules`}
						value={form.rules}
						rows={8}
						onChange={(changeEvent) => setForm({ ...form, rules: changeEvent.target.value })}
					/>
				</Field>
			</form>
		</FormSheet>
	);
}
