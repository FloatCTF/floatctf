/**
 * 「配置」标签：AWDP 赛事配置读写。
 *
 * 已核实的后端语义：
 * - `GET /admin/events/{id}/awdp` 同时返回配置与阶段（阶段来自 active / 最近一次 run）；
 *   非 AWDP 赛制会返回 400（后端 `ensure_awdp_event`）；
 * - PATCH 必须携带 `expected_updated_at`（乐观锁），且在**存在 active run 时锁定配置**；
 * - Break 时长不是独立可配置项：后端按「赛事总时长 − Fix 时长」推导，并把 `events.end_time`
 *   重算为 `start_time + Break + Fix`，因此本表单不提交 break_duration_secs；
 * - Fix 时长会被后端向下取整到回合间隔的整数倍（无 partial round），间隔不能整除会报错。
 */

import { useState, type FormEvent } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Settings2 } from "lucide-react";

import type { AwdpConfigPatchInput, AwdpEventConfigDto } from "@floatctf/sdk";

import { call } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill } from "~/components/app/badges";
import { Field, FormFooter, FormGrid, FormSheet } from "~/components/app/form";
import { KeyValueList, MonoText, SectionCard } from "~/components/app/page";
import { QueryState, TableSkeleton } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import { formatDateTime, formatDuration } from "~/lib/format";

import { useAwdpConfig } from "./queries";
import { AWDP_PHASE_DESCRIPTION, isAwdpConflict, validateFixTiming } from "./shared";

/** 存在 active run 的阶段：后端 `update_config` 在这些阶段锁定配置。 */
const LOCKED_PHASES = ["break", "preparing_fix", "fix"];

interface ConfigForm {
	fix_duration_secs: string;
	fix_round_interval_secs: string;
	break_score: string;
	fix_round_score: string;
}

function formFromConfig(config: AwdpEventConfigDto): ConfigForm {
	return {
		fix_duration_secs: String(config.fix_duration_secs),
		fix_round_interval_secs: String(config.fix_round_interval_secs),
		break_score: String(config.break_score),
		fix_round_score: String(config.fix_round_score),
	};
}

function EditAwdpConfigSheet({
	eventId,
	config,
	onClose,
}: {
	eventId: string;
	config: AwdpEventConfigDto;
	onClose: () => void;
}) {
	const client = useClient();
	const queryClient = useQueryClient();
	const [form, setForm] = useState<ConfigForm>(() => formFromConfig(config));
	const [errors, setErrors] = useState<Partial<Record<keyof ConfigForm, string>>>({});

	const mutation = useMutation({
		mutationFn: (patch: AwdpConfigPatchInput) =>
			call<AwdpEventConfigDto>(client.awdp.admin.updateConfig(eventId, patch), "保存 AWDP 配置"),
		onSuccess: () => {
			toast.success("AWDP 配置已保存", "赛事结束时间已按 开始时间 + Break + Fix 重算。");
			void queryClient.invalidateQueries({ queryKey: qk.awdp.adminConfig(eventId) });
			void queryClient.invalidateQueries({ queryKey: qk.admin.event(eventId) });
			onClose();
		},
		onError: (error) => {
			if (isAwdpConflict(error)) {
				toast.error("配置已被他人修改", "请核对最新配置后重试（已自动重新加载最新配置）。");
				void queryClient.invalidateQueries({ queryKey: qk.awdp.adminConfig(eventId) });
				return;
			}
			toast.apiError("保存 AWDP 配置失败", error);
		},
	});

	const fixDuration = Number(form.fix_duration_secs);
	const interval = Number(form.fix_round_interval_secs);
	const fixScore = Number(form.fix_round_score);
	const timingError = validateFixTiming(fixDuration, interval);
	const rounds = timingError === null ? Math.floor(fixDuration / interval) : null;
	const derivedBreakScore =
		rounds === null ? null : Math.floor((fixScore * rounds * 3) / 5);

	const submit = (event: FormEvent) => {
		event.preventDefault();
		const nextErrors: Partial<Record<keyof ConfigForm, string>> = {};
		if (timingError) {
			if (form.fix_duration_secs.trim() === "" || fixDuration <= 0) {
				nextErrors.fix_duration_secs = "Fix 时长必须为正整数秒";
			}
			if (form.fix_round_interval_secs.trim() === "" || interval <= 0) {
				nextErrors.fix_round_interval_secs = "回合间隔必须为正整数秒";
			}
			if (!nextErrors.fix_duration_secs && !nextErrors.fix_round_interval_secs) {
				nextErrors.fix_duration_secs = timingError;
			}
		}
		for (const key of ["break_score", "fix_round_score"] as const) {
			const raw = form[key].trim();
			if (raw === "" || !/^\d+$/.test(raw)) {
				nextErrors[key] = "必须是非负整数";
				continue;
			}
			if (Number(raw) > 1_000_000_000) nextErrors[key] = "不能超过 1e9";
		}
		setErrors(nextErrors);
		if (Object.keys(nextErrors).length > 0) return;

		const patch: AwdpConfigPatchInput = {};
		if (fixDuration !== config.fix_duration_secs) patch.fix_duration_secs = fixDuration;
		if (interval !== config.fix_round_interval_secs) {
			patch.fix_round_interval_secs = interval;
		}
		if (Number(form.break_score) !== config.break_score) {
			patch.break_score = Number(form.break_score);
		}
		if (fixScore !== config.fix_round_score) patch.fix_round_score = fixScore;
		if (Object.keys(patch).length === 0) {
			toast.warning("没有需要保存的修改");
			return;
		}
		mutation.mutate({ ...patch, expected_updated_at: config.updated_at });
	};

	return (
		<FormSheet
			open
			onOpenChange={(open) => {
				if (!open) onClose();
			}}
			title="修改 AWDP 配置"
			description={`乐观锁版本：${formatDateTime(config.updated_at, { seconds: true })}（提交时一并回传）`}
			width="lg"
			footer={<FormFooter onCancel={onClose} formId="awdp-config-form" isPending={mutation.isPending} />}
		>
			<form id="awdp-config-form" onSubmit={submit} className="space-y-4">
				<FormGrid columns={2}>
					<Field
						label="Break 时长（后端推导）"
						htmlFor="awdp-break-duration"
						hint="= 赛事总时长 − Fix 时长；不可直接指定"
					>
						<Input
							id="awdp-break-duration"
							value={formatDuration(config.break_duration_secs)}
							readOnly
							disabled
						/>
					</Field>
					<Field
						label="Fix 时长（秒）"
						htmlFor="awdp-fix-duration"
						required
						error={errors.fix_duration_secs}
						hint="保存后会被向下取整到回合间隔的整数倍"
					>
						<Input
							id="awdp-fix-duration"
							type="number"
							inputMode="numeric"
							value={form.fix_duration_secs}
							onChange={(event) =>
								setForm((prev) => ({ ...prev, fix_duration_secs: event.target.value }))
							}
						/>
					</Field>
					<Field
						label="回合间隔（秒）"
						htmlFor="awdp-fix-interval"
						required
						error={errors.fix_round_interval_secs}
						hint="每个回合的 cutoff 间隔"
					>
						<Input
							id="awdp-fix-interval"
							type="number"
							inputMode="numeric"
							value={form.fix_round_interval_secs}
							onChange={(event) =>
								setForm((prev) => ({ ...prev, fix_round_interval_secs: event.target.value }))
							}
						/>
					</Field>
					<Field
						label="Break 分值"
						htmlFor="awdp-break-score"
						required
						error={errors.break_score}
						hint={
							derivedBreakScore === null
								? "全部防守成功时的 Break 满分"
								: `按派生规则当前应为 ${derivedBreakScore}（fix_round_score × 回合数 × 0.6）`
						}
					>
						<Input
							id="awdp-break-score"
							type="number"
							inputMode="numeric"
							value={form.break_score}
							onChange={(event) => setForm((prev) => ({ ...prev, break_score: event.target.value }))}
						/>
					</Field>
					<Field
						label="每回合 Fix 分值"
						htmlFor="awdp-fix-score"
						required
						error={errors.fix_round_score}
						hint={rounds === null ? "每个修复回合的得分" : `当前共 ${rounds} 个修复回合`}
					>
						<Input
							id="awdp-fix-score"
							type="number"
							inputMode="numeric"
							value={form.fix_round_score}
							onChange={(event) =>
								setForm((prev) => ({ ...prev, fix_round_score: event.target.value }))
							}
						/>
					</Field>
				</FormGrid>
				<p className="text-xs text-muted-foreground">
					保存会重算赛事结束时间（开始时间 + Break + Fix），并使配置代数 +1；
					新配置只影响之后的比赛，不会改变已产生的成绩。
				</p>
			</form>
		</FormSheet>
	);
}

export function AwdpConfigTab({ eventId }: { eventId: string }) {
	const configQuery = useAwdpConfig(eventId);
	const [editing, setEditing] = useState(false);

	return (
		<QueryState
			query={configQuery}
			skeleton={<TableSkeleton rows={6} columns={3} />}
			errorTitle="加载 AWDP 配置失败（后端要求赛事赛制为 AWDP）"
		>
			{(config) => {
				if (!config) return null;
				const locked = LOCKED_PHASES.includes(config.phase);
				return (
					<>
						<SectionCard
							title="AWDP 配置与运行态"
							description={AWDP_PHASE_DESCRIPTION[config.phase] ?? ""}
							actions={
								<Button
									variant="outline"
									size="sm"
									disabled={locked}
									title={
										locked
											? `比赛进行中（${config.phase}）：后端锁定配置`
											: "修改配置（乐观锁）"
									}
									onClick={() => setEditing(true)}
								>
									<Settings2 /> 修改配置
								</Button>
							}
						>
							<div className="space-y-4">
								<div className="flex flex-wrap items-center gap-2">
									<TonePill tone={locked ? "warning" : "success"}>
										{locked ? "配置已锁定（存在进行中的 run）" : "配置可修改"}
									</TonePill>
									<TonePill tone="neutral">配置代数 {config.configuration_generation}</TonePill>
									<span className="text-xs text-muted-foreground">
										乐观锁版本 updated_at：{formatDateTime(config.updated_at, { seconds: true })}
									</span>
								</div>
								<KeyValueList
									columns={3}
									items={[
										{ key: "Break 时长", value: formatDuration(config.break_duration_secs) },
										{ key: "Fix 时长", value: formatDuration(config.fix_duration_secs) },
										{
											key: "回合间隔",
											value: formatDuration(config.fix_round_interval_secs),
										},
										{ key: "总回合数", value: config.total_rounds },
										{ key: "Break 分值", value: config.break_score },
										{ key: "每回合 Fix 分值", value: config.fix_round_score },
										{ key: "当前回合", value: config.current_round },
										{
											key: "开始时间",
											value: config.started_at
												? formatDateTime(config.started_at, { seconds: true })
												: "未开始",
										},
										{
											key: "Break 结束",
											value: config.break_ends_at
												? formatDateTime(config.break_ends_at, { seconds: true })
												: "—",
										},
										{
											key: "Fix 开始",
											value: config.fix_started_at
												? formatDateTime(config.fix_started_at, { seconds: true })
												: "—",
										},
										{
											key: "Fix 结束",
											value: config.fix_ends_at
												? formatDateTime(config.fix_ends_at, { seconds: true })
												: "—",
										},
										{
											key: "结束时间",
											value: config.finished_at
												? formatDateTime(config.finished_at, { seconds: true })
												: "未结束",
										},
										{
											key: "下一动作时间",
											value: config.next_action_at
												? formatDateTime(config.next_action_at, { seconds: true })
												: "—",
										},
										{ key: "赛事 ID", value: <MonoText>{config.event_id}</MonoText> },
									]}
								/>
							</div>
						</SectionCard>
						{editing ? (
							<EditAwdpConfigSheet
								eventId={eventId}
								config={config}
								onClose={() => setEditing(false)}
							/>
						) : null}
					</>
				);
			}}
		</QueryState>
	);
}

/** 非 AWDP 赛制时后端返回 400：这里给出更明确的解释（避免看起来像「加载失败」）。 */
