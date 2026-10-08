/**
 * 「配置」标签：AWD 赛事配置的读取 / 首次开通 / 修改。
 *
 * 已核实的后端语义：
 * - 首次开通走 `POST /admin/events/awd`（body 需 event_id；赛事必须是 AWD 赛制、且有结束时间）；
 * - 修改走 PATCH，**必须**携带 `expected_updated_at`（乐观锁），并至少提交一个变更字段；
 * - 运行态（running/paused/network_error/finished/archived）配置锁定；prechecking 期间也不可改。
 */

import { useState, type FormEvent } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Settings2 } from "lucide-react";

import type { AwdEventConfigInput, AwdEventStatus } from "@floatctf/sdk";

import { call } from "~/api/call";
import { useClient } from "~/api/client";
import { qk } from "~/api/keys";
import { TonePill } from "~/components/app/badges";
import { Field, FormFooter, FormGrid, FormSheet } from "~/components/app/form";
import { KeyValueList, MonoText, SectionCard } from "~/components/app/page";
import { EmptyBlock, QueryState, TableSkeleton } from "~/components/app/states";
import { toast } from "~/components/app/toast";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import { formatDateTime, fromDatetimeLocalValue, toDatetimeLocalValue } from "~/lib/format";

import { useAwdEvent, useAwdStatus } from "./queries";
import { isConflictError } from "./shared";

type NumericKey =
	| "round_count"
	| "round_duration_secs"
	| "initial_score"
	| "free_reset_count"
	| "extra_reset_penalty"
	| "judge_max_concurrency"
	| "judge_default_timeout_secs"
	| "judge_retry_interval_secs"
	| "archive_retention_hours";

interface NumericField {
	key: NumericKey;
	label: string;
	min: number;
	max: number;
	hint: string;
}

/** 范围与后端 `AwdEventConfigPatch::validate()` 一致（超范围后端会 400）。 */
const NUMERIC_FIELDS: NumericField[] = [
	{ key: "round_count", label: "总轮次", min: 1, max: 10_000, hint: "1 – 10000" },
	{
		key: "round_duration_secs",
		label: "单轮时长（秒）",
		min: 30,
		max: 86_400,
		hint: "30 – 86400 秒",
	},
	{ key: "initial_score", label: "初始分数", min: 0, max: 1_000_000_000, hint: "0 – 1e9" },
	{ key: "free_reset_count", label: "免费重置次数", min: 0, max: 100, hint: "0 – 100" },
	{
		key: "extra_reset_penalty",
		label: "额外重置罚分",
		min: 0,
		max: 1_000_000_000,
		hint: "0 – 1e9",
	},
	{ key: "judge_max_concurrency", label: "判题最大并发", min: 1, max: 1_000, hint: "1 – 1000" },
	{
		key: "judge_default_timeout_secs",
		label: "默认判题超时（秒）",
		min: 1,
		max: 3_600,
		hint: "1 – 3600",
	},
	{
		key: "judge_retry_interval_secs",
		label: "判题重试间隔（秒）",
		min: 1,
		max: 3_600,
		hint: "1 – 3600",
	},
	{
		key: "archive_retention_hours",
		label: "归档保留（小时）",
		min: 1,
		max: 87_600,
		hint: "1 – 87600",
	},
];

/** 后端 `assert_config_editable` 允许改配置的状态（其余状态配置锁定）。 */
const EDITABLE_STATUSES = [
	"draft",
	"configuring",
	"deployed",
	"verified",
	"start_blocked",
	"deploy_failed",
	"verification_failed",
];

type ConfigFormState = Record<NumericKey, string> & { planned_start_at: string };

function emptyForm(): ConfigFormState {
	const base = { planned_start_at: "" } as ConfigFormState;
	for (const field of NUMERIC_FIELDS) base[field.key] = "";
	return base;
}

function formFromStatus(status: AwdEventStatus): ConfigFormState {
	const form = emptyForm();
	for (const field of NUMERIC_FIELDS) {
		const value = status[field.key];
		form[field.key] = value === null || value === undefined ? "" : String(value);
	}
	form.planned_start_at = toDatetimeLocalValue(status.planned_start_at);
	return form;
}

type FormErrors = Partial<Record<NumericKey | "planned_start_at", string>>;

function validateForm(form: ConfigFormState): FormErrors {
	const errors: FormErrors = {};
	for (const field of NUMERIC_FIELDS) {
		const raw = form[field.key].trim();
		if (raw === "") continue;
		if (!/^-?\d+$/.test(raw)) {
			errors[field.key] = "必须是整数";
			continue;
		}
		const value = Number(raw);
		if (value < field.min || value > field.max) {
			errors[field.key] = `需在 ${field.min} – ${field.max} 之间`;
		}
	}
	if (form.planned_start_at !== "") {
		const iso = fromDatetimeLocalValue(form.planned_start_at);
		if (iso === null) errors.planned_start_at = "时间格式无法解析";
		else if (new Date(iso).getTime() <= Date.now()) {
			errors.planned_start_at = "计划开赛时间必须晚于当前时间";
		}
	}
	return errors;
}

function collectNumbers(form: ConfigFormState): Partial<Record<NumericKey, number>> {
	const out: Partial<Record<NumericKey, number>> = {};
	for (const field of NUMERIC_FIELDS) {
		const raw = form[field.key].trim();
		if (raw === "") continue;
		out[field.key] = Number(raw);
	}
	return out;
}

/** 相对已加载配置计算 PATCH 最小变更集；无变更返回 null。 */
function buildPatch(status: AwdEventStatus, form: ConfigFormState): AwdEventConfigInput | null {
	const patch: AwdEventConfigInput = {};
	for (const field of NUMERIC_FIELDS) {
		const raw = form[field.key].trim();
		if (raw === "") continue;
		const value = Number(raw);
		if (value !== status[field.key]) patch[field.key] = value;
	}
	const originalPlanned = toDatetimeLocalValue(status.planned_start_at);
	if (form.planned_start_at !== originalPlanned) {
		if (form.planned_start_at === "") {
			// 原本有排期、现在清空 → 删除已排期的开赛任务。
			if (originalPlanned !== "") patch.clear_planned_start = true;
		} else {
			const iso = fromDatetimeLocalValue(form.planned_start_at);
			if (iso) patch.planned_start_at = iso;
		}
	}
	return Object.keys(patch).length > 0 ? patch : null;
}

function ConfigFields({
	form,
	errors,
	onChange,
	disabled,
}: {
	form: ConfigFormState;
	errors: FormErrors;
	onChange: (key: keyof ConfigFormState, value: string) => void;
	disabled: boolean;
}) {
	return (
		<FormGrid columns={2}>
			{NUMERIC_FIELDS.map((field) => (
				<Field
					key={field.key}
					label={field.label}
					htmlFor={`awd-${field.key}`}
					hint={field.hint}
					error={errors[field.key]}
				>
					<Input
						id={`awd-${field.key}`}
						type="number"
						inputMode="numeric"
						value={form[field.key]}
						disabled={disabled}
						onChange={(event) => onChange(field.key, event.target.value)}
					/>
				</Field>
			))}
			<Field
				label="计划开赛时间"
				htmlFor="awd-planned-start"
				hint="留空 = 手动开赛；清空已有排期会删除定时开赛任务"
				error={errors.planned_start_at}
				className="sm:col-span-2"
			>
				<Input
					id="awd-planned-start"
					type="datetime-local"
					value={form.planned_start_at}
					disabled={disabled}
					onChange={(event) => onChange("planned_start_at", event.target.value)}
				/>
			</Field>
		</FormGrid>
	);
}

/** 首次开通：POST /admin/events/awd（body 带 event_id）。 */
function CreateAwdSheet({
	eventId,
	family,
	onClose,
}: {
	eventId: string;
	family: string;
	onClose: () => void;
}) {
	const client = useClient();
	const queryClient = useQueryClient();
	const [form, setForm] = useState<ConfigFormState>(emptyForm);
	const [errors, setErrors] = useState<FormErrors>({});

	const notAwd = family !== "awd";

	const mutation = useMutation({
		mutationFn: (body: AwdEventConfigInput & { event_id: string }) =>
			call<string>(client.awd.admin.createEvent(body), "开通 AWD"),
		onSuccess: () => {
			toast.success("AWD 已开通", "接下来请在「网络」标签页分配赛事网络，然后部署。");
			void queryClient.invalidateQueries({ queryKey: qk.awd.adminStatus(eventId) });
			onClose();
		},
		onError: (error) => toast.apiError("开通 AWD 失败", error),
	});

	const submit = (event: FormEvent) => {
		event.preventDefault();
		const nextErrors = validateForm(form);
		setErrors(nextErrors);
		if (Object.keys(nextErrors).length > 0) return;
		const body: AwdEventConfigInput & { event_id: string } = {
			event_id: eventId,
			...collectNumbers(form),
		};
		const planned = fromDatetimeLocalValue(form.planned_start_at);
		if (planned) body.planned_start_at = planned;
		mutation.mutate(body);
	};

	return (
		<FormSheet
			open
			onOpenChange={(open) => {
				if (!open) onClose();
			}}
			title="开通 AWD"
			description="为该赛事创建 AWD 配置。留空的字段使用后端默认值；开通后需分配赛事网络才能部署。"
			width="lg"
			footer={
				<FormFooter
					onCancel={onClose}
					formId="awd-create-form"
					submitLabel="开通"
					isPending={mutation.isPending}
					disabled={notAwd}
					hint={notAwd ? "该赛事不是 AWD 赛制，无法开通" : undefined}
				/>
			}
		>
			<form id="awd-create-form" onSubmit={submit} className="space-y-4">
				{notAwd ? (
					<p className="text-sm text-destructive">
						该赛事的赛制为「{family || "未知"}」，后端只接受 AWD 赛制的赛事。
					</p>
				) : (
					<ConfigFields
						form={form}
						errors={errors}
						disabled={mutation.isPending}
						onChange={(key, value) => setForm((prev) => ({ ...prev, [key]: value }))}
					/>
				)}
			</form>
		</FormSheet>
	);
}

/** 修改配置：PATCH + expected_updated_at 乐观锁。 */
function EditAwdSheet({
	eventId,
	status,
	onClose,
}: {
	eventId: string;
	status: AwdEventStatus;
	onClose: () => void;
}) {
	const client = useClient();
	const queryClient = useQueryClient();
	const [form, setForm] = useState<ConfigFormState>(() => formFromStatus(status));
	const [errors, setErrors] = useState<FormErrors>({});

	const mutation = useMutation({
		mutationFn: (patch: AwdEventConfigInput) =>
			call<AwdEventStatus>(
				client.awd.admin.updateConfig(eventId, {
					...patch,
					// 乐观锁：回传读取时拿到的 updated_at，避免覆盖他人修改。
					expected_updated_at: status.updated_at,
				}),
				"保存 AWD 配置",
			),
		onSuccess: () => {
			toast.success("AWD 配置已保存");
			void queryClient.invalidateQueries({ queryKey: qk.awd.adminStatus(eventId) });
			onClose();
		},
		onError: (error) => {
			if (isConflictError(error)) {
				toast.error("配置已被他人修改", "请核对最新配置后重试（已自动重新加载最新配置）。");
				void queryClient.invalidateQueries({ queryKey: qk.awd.adminStatus(eventId) });
				return;
			}
			toast.apiError("保存 AWD 配置失败", error);
		},
	});

	const submit = (event: FormEvent) => {
		event.preventDefault();
		const nextErrors = validateForm(form);
		setErrors(nextErrors);
		if (Object.keys(nextErrors).length > 0) return;
		const patch = buildPatch(status, form);
		if (!patch) {
			toast.warning("没有需要保存的修改", "至少修改一个字段后才能提交。");
			return;
		}
		mutation.mutate(patch);
	};

	return (
		<FormSheet
			open
			onOpenChange={(open) => {
				if (!open) onClose();
			}}
			title="修改 AWD 配置"
			description={`乐观锁版本：${formatDateTime(status.updated_at, { seconds: true })}（提交时会一并回传）`}
			width="lg"
			footer={<FormFooter onCancel={onClose} formId="awd-edit-form" isPending={mutation.isPending} />}
		>
			<form id="awd-edit-form" onSubmit={submit} className="space-y-4">
				<ConfigFields
					form={form}
					errors={errors}
					disabled={mutation.isPending}
					onChange={(key, value) => setForm((prev) => ({ ...prev, [key]: value }))}
				/>
				<p className="text-xs text-muted-foreground">
					运行时参数变更会使赛事回到「配置中」、配置代数 +1 并清除预检结果，需要重新预检。
				</p>
			</form>
		</FormSheet>
	);
}

export function AwdConfigTab({ eventId }: { eventId: string }) {
	const statusQuery = useAwdStatus(eventId);
	const eventQuery = useAwdEvent(eventId);
	const [creating, setCreating] = useState(false);
	const [editing, setEditing] = useState(false);

	const family = eventQuery.data?.family ?? "";

	// 首次开通时后端要求赛事为 AWD 赛制（family 未知时不禁用，交给后端裁决并展示错误）。
	const familyBlocked = family !== "" && family !== "awd";

	return (
		<>
			<QueryState
				query={statusQuery}
				skeleton={<TableSkeleton rows={6} columns={3} />}
				empty={
				<EmptyBlock
					title="尚未开通 AWD"
					description="该赛事还没有 AWD 配置。开通后即可分配赛事网络、部署运行时并预检。"
					icon={<Settings2 className="size-5" />}
					action={
						<Button
							size="sm"
							disabled={familyBlocked}
							title={familyBlocked ? `该赛事赛制为「${family}」，不是 AWD` : undefined}
							onClick={() => setCreating(true)}
						>
							开通 AWD
						</Button>
					}
				/>
			}
		>
			{(data) => {
				// QueryState 已在 data == null 时渲染 empty；这里只做类型收窄。
				if (!data) return null;
				const editable = EDITABLE_STATUSES.includes(data.status);
				return (
					<>
						<SectionCard
							title="AWD 运行时配置"
							description="范围由后端校验；保存会带回乐观锁版本。"
							actions={
								<Button
									variant="outline"
									size="sm"
									disabled={!editable}
									title={editable ? undefined : `状态「${data.status}」下配置已锁定`}
									onClick={() => setEditing(true)}
								>
									<Settings2 /> 修改配置
								</Button>
							}
						>
							<div className="space-y-4">
								<div className="flex flex-wrap items-center gap-2">
									<TonePill tone={editable ? "success" : "warning"}>
										{editable ? "配置可修改" : "配置已锁定"}
									</TonePill>
									<span className="text-xs text-muted-foreground">
										乐观锁版本 updated_at：{formatDateTime(data.updated_at, { seconds: true })}
									</span>
								</div>
								<KeyValueList
									columns={3}
									items={[
										{ key: "总轮次", value: data.round_count ?? "—" },
										{
											key: "单轮时长",
											value: `${data.round_duration_secs} 秒`,
										},
										{ key: "初始分数", value: data.initial_score },
										{ key: "免费重置次数", value: data.free_reset_count },
										{ key: "额外重置罚分", value: data.extra_reset_penalty },
										{ key: "判题最大并发", value: data.judge_max_concurrency },
										{
											key: "默认判题超时",
											value: `${data.judge_default_timeout_secs} 秒`,
										},
										{
											key: "判题重试间隔",
											value: `${data.judge_retry_interval_secs} 秒`,
										},
										{ key: "归档保留", value: `${data.archive_retention_hours} 小时` },
										{
											key: "计划开赛",
											value: data.planned_start_at ? (
												formatDateTime(data.planned_start_at, { seconds: true })
											) : (
												<span className="text-muted-foreground">手动开赛</span>
											),
										},
										{
											key: "预检通过时间",
											value: data.verified_at ? (
												formatDateTime(data.verified_at, { seconds: true })
											) : (
												<span className="text-muted-foreground">未通过</span>
											),
										},
										{
											key: "实际开赛时间",
											value: data.started_at ? (
												formatDateTime(data.started_at, { seconds: true })
											) : (
												<span className="text-muted-foreground">未开赛</span>
											),
										},
										{ key: "赛事 ID", value: <MonoText>{data.event_id}</MonoText> },
									]}
								/>
								{!editable ? (
									<p className="text-xs text-muted-foreground">
										当前状态（{data.status}）下后端拒绝修改配置：仅草稿 / 配置中 / 已部署 /
										开赛受阻 / 部署失败 / 预检失败 / 预检通过 可改。
									</p>
								) : null}
							</div>
						</SectionCard>

						{editing ? (
							<EditAwdSheet eventId={eventId} status={data} onClose={() => setEditing(false)} />
						) : null}
					</>
				);
			}}
			</QueryState>

			{/* 创建表单必须在 QueryState 之外：未开通时 QueryState 渲染的是空态，而不是 children。 */}
			{creating ? (
				<CreateAwdSheet eventId={eventId} family={family} onClose={() => setCreating(false)} />
			) : null}
		</>
	);
}
