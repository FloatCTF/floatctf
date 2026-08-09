import {
	Box,
	Button,
	FormControl,
	Label,
	Spinner,
	TextInput,
} from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { type ChangeEvent, useEffect, useRef, useState } from "react";

import { adminApi } from "@/api";
import type { AwdEventConfigInput } from "@/api/awd";
import { useMsgBanner } from "@/components";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/awd/$id/configure")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

type FormState = {
	roundDurationSecs: string;
	freeResetCount: string;
	extraResetPenalty: string;
	resetProtectionSecs: string;
	judgeMaxConcurrency: string;
	judgeDefaultTimeoutSecs: string;
	judgeRetryIntervalSecs: string;
	judgeGracePeriodSecs: string;
	archiveRetentionHours: string;
	plannedStartAt: string;
};

const DEFAULT_FORM: FormState = {
	roundDurationSecs: "300",
	freeResetCount: "3",
	extraResetPenalty: "100",
	resetProtectionSecs: "120",
	judgeMaxConcurrency: "10",
	judgeDefaultTimeoutSecs: "30",
	judgeRetryIntervalSecs: "5",
	judgeGracePeriodSecs: "30",
	archiveRetentionHours: "168",
	plannedStartAt: "",
};

const EDITABLE_STATUSES = new Set([
	"draft",
	"configuring",
	"deployed",
	"verified",
	"start_blocked",
	"deploy_failed",
	"verification_failed",
]);

function toLocalDateTimeInput(value: string | null): string {
	if (!value) return "";
	const date = new Date(value);
	const local = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
	return local.toISOString().slice(0, 16);
}

function RouteComponent() {
	const { id } = Route.useParams();
	const qc = useQueryClient();
	const banner = useMsgBanner({});
	const [form, setForm] = useState<FormState>(DEFAULT_FORM);
	const [dirty, setDirty] = useState(false);
	const loadedVersion = useRef<string | null>(null);

	const statusQuery = useQuery({
		queryKey: ["admin-awd-status", id],
		queryFn: () => adminApi.awd.getStatus(id),
	});
	const config = statusQuery.data?.data ?? null;

	useEffect(() => {
		if (!config) {
			if (!dirty && loadedVersion.current !== "unconfigured") {
				setForm(DEFAULT_FORM);
				loadedVersion.current = "unconfigured";
			}
			return;
		}
		if (loadedVersion.current === config.updated_at) return;
		if (dirty && loadedVersion.current !== null) return;
		setForm({
			roundDurationSecs: String(config.round_duration_secs),
			freeResetCount: String(config.free_reset_count),
			extraResetPenalty: String(config.extra_reset_penalty),
			resetProtectionSecs: String(config.reset_protection_secs),
			judgeMaxConcurrency: String(config.judge_max_concurrency),
			judgeDefaultTimeoutSecs: String(config.judge_default_timeout_secs),
			judgeRetryIntervalSecs: String(config.judge_retry_interval_secs),
			judgeGracePeriodSecs: String(config.judge_grace_period_secs),
			archiveRetentionHours: String(config.archive_retention_hours),
			plannedStartAt: toLocalDateTimeInput(config.planned_start_at),
		});
		loadedVersion.current = config.updated_at;
	}, [config, dirty]);

	const save = useMutation({
		mutationFn: async (payload: AwdEventConfigInput) => {
			if (config) {
				await adminApi.awd.updateConfig(id, payload);
			} else {
				await adminApi.awd.createEvent({ event_id: id, ...payload });
			}
		},
		onSuccess: () => {
			setDirty(false);
			banner.showBanner("success", "AWD configuration saved");
			qc.invalidateQueries({ queryKey: ["admin-awd-status", id] });
			qc.invalidateQueries({ queryKey: ["event", id] });
		},
		onError: (error) => {
			banner.showErrorBanner(error);
			void statusQuery.refetch();
		},
	});

	if (statusQuery.isLoading) return <Spinner size="large" />;
	if (statusQuery.isError) {
		return <div>Failed to load AWD configuration.</div>;
	}

	const editable = !config || EDITABLE_STATUSES.has(config.status);
	const remoteChanged = Boolean(
		config &&
			loadedVersion.current !== null &&
			loadedVersion.current !== "unconfigured" &&
			loadedVersion.current !== config.updated_at,
	);
	const formReady = !config || loadedVersion.current === config.updated_at;
	const set =
		(key: keyof FormState) => (event: ChangeEvent<HTMLInputElement>) => {
			setDirty(true);
			setForm((current) => ({ ...current, [key]: event.target.value }));
		};
	const submit = () => {
		const fields = [
			form.roundDurationSecs,
			form.freeResetCount,
			form.extraResetPenalty,
			form.resetProtectionSecs,
			form.judgeMaxConcurrency,
			form.judgeDefaultTimeoutSecs,
			form.judgeRetryIntervalSecs,
			form.judgeGracePeriodSecs,
			form.archiveRetentionHours,
		].map(Number);
		if (fields.some((value) => !Number.isSafeInteger(value))) {
			banner.showBanner(
				"critical",
				"All numeric fields must be valid integers.",
			);
			return;
		}
		const ranges: Array<[number, number, number, string]> = [
			[fields[0], 30, 86_400, "Round Duration"],
			[fields[1], 0, 100, "Free Reset Count"],
			[fields[2], 0, 1_000_000_000, "Extra Reset Penalty"],
			[fields[3], 0, 86_400, "Reset Protection"],
			[fields[4], 1, 1_000, "Max Concurrency"],
			[fields[5], 1, 3_600, "Default Timeout"],
			[fields[6], 1, 3_600, "Retry Interval"],
			[fields[7], 0, 3_600, "Grace Period"],
			[fields[8], 1, 87_600, "Archive Retention"],
		];
		const invalidRange = ranges.find(
			([value, min, max]) => value < min || value > max,
		);
		if (invalidRange) {
			banner.showBanner(
				"critical",
				`${invalidRange[3]} must be between ${invalidRange[1]} and ${invalidRange[2]}.`,
			);
			return;
		}
		const [
			roundDurationSecs,
			freeResetCount,
			extraResetPenalty,
			resetProtectionSecs,
			judgeMaxConcurrency,
			judgeDefaultTimeoutSecs,
			judgeRetryIntervalSecs,
			judgeGracePeriodSecs,
			archiveRetentionHours,
		] = fields;
		const payload: AwdEventConfigInput = {
			expected_updated_at:
				loadedVersion.current === "unconfigured"
					? undefined
					: (loadedVersion.current ?? undefined),
			round_duration_secs: roundDurationSecs,
			free_reset_count: freeResetCount,
			extra_reset_penalty: extraResetPenalty,
			reset_protection_secs: resetProtectionSecs,
			judge_max_concurrency: judgeMaxConcurrency,
			judge_default_timeout_secs: judgeDefaultTimeoutSecs,
			judge_retry_interval_secs: judgeRetryIntervalSecs,
			judge_grace_period_secs: judgeGracePeriodSecs,
			archive_retention_hours: archiveRetentionHours,
			clear_planned_start: !form.plannedStartAt,
		};
		if (form.plannedStartAt) {
			const plannedStart = new Date(form.plannedStartAt);
			if (
				!Number.isFinite(plannedStart.getTime()) ||
				plannedStart <= new Date()
			) {
				banner.showBanner(
					"critical",
					"Planned Start must be a valid future time.",
				);
				return;
			}
			payload.planned_start_at = plannedStart.toISOString();
		}
		save.mutate(payload);
	};

	return (
		<div className="mt-3" style={{ maxWidth: 920 }}>
			<banner.BannerComponent className="mb-3" />
			<Box
				sx={{
					p: 4,
					border: "1px solid",
					borderColor: "border.default",
					borderRadius: 2,
				}}
			>
				<div className="d-flex flex-items-center flex-justify-between">
					<div>
						<h3 className="m-0">AWD Configure</h3>
						<p className="color-fg-muted mb-0 mt-1">
							首次保存会创建 AWD 配置与内部密钥；之后在这里统一维护轮次、
							Reset、Judge 和归档参数。
						</p>
					</div>
					<Label variant={config ? "success" : "accent"}>
						{config ? config.status : "not configured"}
					</Label>
				</div>

				{remoteChanged && (
					<Box sx={{ mt: 3, p: 3, bg: "attention.subtle", borderRadius: 2 }}>
						The server configuration changed while this form had unsaved edits.
						<Button
							className="ml-2"
							onClick={() => {
								loadedVersion.current = null;
								setDirty(false);
							}}
						>
							Reload server values
						</Button>
					</Box>
				)}

				{!editable && (
					<Box
						sx={{
							mt: 3,
							p: 3,
							bg: "attention.subtle",
							borderRadius: 2,
						}}
					>
						当前状态为 <strong>{config?.status}</strong>，AWD 参数已锁定。
						请先结束当前运行操作或回到可配置状态。
					</Box>
				)}

				<Section title="Match & Schedule">
					<NumberField
						label="Round Duration"
						caption="每轮时长（秒），30–86400。"
						value={form.roundDurationSecs}
						onChange={set("roundDurationSecs")}
						min={30}
						max={86400}
						disabled={!editable}
					/>
					<FormControl disabled={!editable}>
						<FormControl.Label>Planned Start</FormControl.Label>
						<FormControl.Caption>
							可选；留空表示在 Ops 页面手动 Start。
						</FormControl.Caption>
						<TextInput
							type="datetime-local"
							value={form.plannedStartAt}
							onChange={set("plannedStartAt")}
						/>
					</FormControl>
				</Section>

				<Section title="Reset Policy">
					<NumberField
						label="Free Reset Count"
						caption="每队免费 Reset 次数。"
						value={form.freeResetCount}
						onChange={set("freeResetCount")}
						min={0}
						max={100}
						disabled={!editable}
					/>
					<NumberField
						label="Extra Reset Penalty"
						caption="超出免费次数后的扣分。"
						value={form.extraResetPenalty}
						onChange={set("extraResetPenalty")}
						min={0}
						max={1_000_000_000}
						disabled={!editable}
					/>
					<NumberField
						label="Reset Protection"
						caption="Reset 后保护时间（秒）。"
						value={form.resetProtectionSecs}
						onChange={set("resetProtectionSecs")}
						min={0}
						max={86400}
						disabled={!editable}
					/>
				</Section>

				<Section title="Judge Policy">
					<NumberField
						label="Max Concurrency"
						caption="单赛事 Judge 最大并发数。"
						value={form.judgeMaxConcurrency}
						onChange={set("judgeMaxConcurrency")}
						min={1}
						max={1000}
						disabled={!editable}
					/>
					<NumberField
						label="Default Timeout"
						caption="Judge 默认超时（秒）。"
						value={form.judgeDefaultTimeoutSecs}
						onChange={set("judgeDefaultTimeoutSecs")}
						min={1}
						max={3600}
						disabled={!editable}
					/>
					<NumberField
						label="Retry Interval"
						caption="Judge 重试间隔（秒）。"
						value={form.judgeRetryIntervalSecs}
						onChange={set("judgeRetryIntervalSecs")}
						min={1}
						max={3600}
						disabled={!editable}
					/>
					<NumberField
						label="Grace Period"
						caption="Judge 结果宽限期（秒）。"
						value={form.judgeGracePeriodSecs}
						onChange={set("judgeGracePeriodSecs")}
						min={0}
						max={3600}
						disabled={!editable}
					/>
				</Section>

				<Section title="Lifecycle">
					<NumberField
						label="Archive Retention"
						caption="Finished 后保留小时数。"
						value={form.archiveRetentionHours}
						onChange={set("archiveRetentionHours")}
						min={1}
						max={87600}
						disabled={!editable}
					/>
				</Section>

				<Box sx={{ mt: 4 }}>
					<Button
						variant="primary"
						disabled={
							!editable || !formReady || remoteChanged || save.isPending
						}
						onClick={submit}
					>
						{save.isPending
							? "Saving…"
							: config
								? "Save AWD Configuration"
								: "Create AWD Configuration"}
					</Button>
				</Box>
			</Box>
		</div>
	);
}

function Section({
	title,
	children,
}: { title: string; children: React.ReactNode }) {
	return (
		<section className="mt-4">
			<h4 className="mb-2">{title}</h4>
			<div
				style={{
					display: "grid",
					gridTemplateColumns: "repeat(auto-fit, minmax(260px, 1fr))",
					gap: 16,
				}}
			>
				{children}
			</div>
		</section>
	);
}

type NumberFieldProps = {
	label: string;
	caption: string;
	value: string;
	onChange: (event: ChangeEvent<HTMLInputElement>) => void;
	min: number;
	max: number;
	disabled: boolean;
};

function NumberField({
	label,
	caption,
	value,
	onChange,
	min,
	max,
	disabled,
}: NumberFieldProps) {
	return (
		<FormControl disabled={disabled}>
			<FormControl.Label>{label}</FormControl.Label>
			<FormControl.Caption>{caption}</FormControl.Caption>
			<TextInput
				type="number"
				value={value}
				onChange={onChange}
				min={min}
				max={max}
				step={1}
				required
				block
			/>
		</FormControl>
	);
}
