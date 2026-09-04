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
	roundCount: string;
	roundDurationSecs: string;
	initialScore: string;
	freeResetCount: string;
	extraResetPenalty: string;
	judgeMaxConcurrency: string;
	judgeDefaultTimeoutSecs: string;
	judgeRetryIntervalSecs: string;
	archiveRetentionHours: string;
	plannedStartAt: string;
};

const DEFAULT_FORM: FormState = {
	roundCount: "10",
	roundDurationSecs: "300",
	initialScore: "1000",
	freeResetCount: "3",
	extraResetPenalty: "100",
	judgeMaxConcurrency: "10",
	judgeDefaultTimeoutSecs: "30",
	judgeRetryIntervalSecs: "5",
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
	const eventQuery = useQuery({
		queryKey: ["event", id],
		queryFn: () => adminApi.events.get(id),
	});
	const config = statusQuery.data?.data ?? null;
	const event = eventQuery.data?.data;

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
			roundCount: config.round_count != null ? String(config.round_count) : "10",
			roundDurationSecs: String(config.round_duration_secs),
			initialScore: String(config.initial_score),
			freeResetCount: String(config.free_reset_count),
			extraResetPenalty: String(config.extra_reset_penalty),
			judgeMaxConcurrency: String(config.judge_max_concurrency),
			judgeDefaultTimeoutSecs: String(config.judge_default_timeout_secs),
			judgeRetryIntervalSecs: String(config.judge_retry_interval_secs),
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

	if (statusQuery.isLoading || eventQuery.isLoading) return <Spinner size="large" />;
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
		const numericFields = [
			form.roundCount,
			form.roundDurationSecs,
			form.initialScore,
			form.freeResetCount,
			form.extraResetPenalty,
			form.judgeMaxConcurrency,
			form.judgeDefaultTimeoutSecs,
			form.judgeRetryIntervalSecs,
			form.archiveRetentionHours,
		].map(Number);
		if (numericFields.some((value) => !Number.isSafeInteger(value))) {
			banner.showBanner("critical", "All numeric fields must be valid integers.");
			return;
		}
		const ranges: Array<[number, number, number, string]> = [
			[numericFields[0], 1, 1000, "Round Count"],
			[numericFields[1], 30, 86_400, "Round Duration"],
			[numericFields[2], 0, 1_000_000_000, "Initial Score"],
			[numericFields[3], 0, 100, "Free Reset Count"],
			[numericFields[4], 0, 1_000_000_000, "Extra Reset Penalty"],
			[numericFields[5], 1, 1_000, "Max Concurrency"],
			[numericFields[6], 1, 3_600, "Default Timeout"],
			[numericFields[7], 1, 3_600, "Retry Interval"],
			[numericFields[8], 1, 87_600, "Archive Retention"],
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
			roundCount,
			roundDurationSecs,
			initialScore,
			freeResetCount,
			extraResetPenalty,
			judgeMaxConcurrency,
			judgeDefaultTimeoutSecs,
			judgeRetryIntervalSecs,
			archiveRetentionHours,
		] = numericFields;
		const payload: AwdEventConfigInput = {
			expected_updated_at:
				loadedVersion.current === "unconfigured"
					? undefined
					: (loadedVersion.current ?? undefined),
			round_count: roundCount,
			round_duration_secs: roundDurationSecs,
			initial_score: initialScore,
			free_reset_count: freeResetCount,
			extra_reset_penalty: extraResetPenalty,
			judge_max_concurrency: judgeMaxConcurrency,
			judge_default_timeout_secs: judgeDefaultTimeoutSecs,
			judge_retry_interval_secs: judgeRetryIntervalSecs,
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

	// ── Timing preview ──
	const roundCount = Number.parseInt(form.roundCount, 10) || 0;
	const roundDur = Number.parseInt(form.roundDurationSecs, 10) || 0;
	const attackDuration = roundCount * roundDur;
	const eventStart = event?.start_time ? new Date(event.start_time).getTime() : null;
	const eventEnd = event?.end_time ? new Date(event.end_time).getTime() : null;
	const eventDuration = eventStart && eventEnd ? (eventEnd - eventStart) / 1000 : null;
	const hardeningDuration = eventDuration != null ? eventDuration - attackDuration : null;
	const timingValid = attackDuration > 0 && eventDuration != null && attackDuration <= eventDuration;

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
							Configure rounds, scoring, Reset policy, Judge, and lifecycle parameters.
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
						Current status is <strong>{config?.status}</strong> — AWD parameters are locked.
					</Box>
				)}

				{/* Timing Preview */}
				{eventDuration != null && roundCount > 0 && roundDur > 0 && (
					<Box
						sx={{
							mt: 3,
							p: 3,
							bg: timingValid ? "success.subtle" : "danger.subtle",
							borderRadius: 2,
						}}
					>
						<h4 className="mb-2">Timing Preview</h4>
						<div className="grid grid-cols-2 gap-2 text-sm">
							<span className="text-[var(--fgColor-muted)]">Event Duration</span>
							<span className="font-mono">{formatDuration(eventDuration)}</span>
							<span className="text-[var(--fgColor-muted)]">Attack Duration</span>
							<span className="font-mono">
								{roundCount} rounds × {roundDur}s = {formatDuration(attackDuration)}
							</span>
							<span className="text-[var(--fgColor-muted)]">Hardening Duration</span>
							<span className="font-mono">
								{hardeningDuration != null && hardeningDuration >= 0
									? formatDuration(hardeningDuration)
									: "N/A"}
							</span>
						</div>
						{!timingValid && (
							<p className="text-sm color-fg-danger mt-2">
								⚠ Attack duration ({roundCount} × {roundDur}s = {formatDuration(attackDuration)}) exceeds event duration ({formatDuration(eventDuration)}). Configuration will be rejected.
							</p>
						)}
					</Box>
				)}

				<Section title="Match & Schedule">
					<NumberField
						label="Round Count"
						caption="Total number of Attack rounds (1–1000)."
						value={form.roundCount}
						onChange={set("roundCount")}
						min={1}
						max={1000}
						disabled={!editable}
					/>
					<NumberField
						label="Round Duration"
						caption="Seconds per round (30–86400)."
						value={form.roundDurationSecs}
						onChange={set("roundDurationSecs")}
						min={30}
						max={86400}
						disabled={!editable}
					/>
					<FormControl disabled={!editable}>
						<FormControl.Label>Planned Start</FormControl.Label>
						<FormControl.Caption>
							Optional; leave empty for manual Start from Operations.
						</FormControl.Caption>
						<TextInput
							type="datetime-local"
							value={form.plannedStartAt}
							onChange={set("plannedStartAt")}
						/>
					</FormControl>
				</Section>

				<Section title="Scoring">
					<NumberField
						label="Initial Score"
						caption="Starting score for each team (0–1,000,000,000)."
						value={form.initialScore}
						onChange={set("initialScore")}
						min={0}
						max={1_000_000_000}
						disabled={!editable}
					/>
				</Section>

				<Section title="Reset Policy">
					<NumberField
						label="Free Reset Count"
						caption="Number of free Resets per team."
						value={form.freeResetCount}
						onChange={set("freeResetCount")}
						min={0}
						max={100}
						disabled={!editable}
					/>
					<NumberField
						label="Extra Reset Penalty"
						caption="Score penalty per extra Reset."
						value={form.extraResetPenalty}
						onChange={set("extraResetPenalty")}
						min={0}
						max={1_000_000_000}
						disabled={!editable}
					/>
				</Section>

				<Section title="Judge Policy">
					<NumberField
						label="Max Concurrency"
						caption="Maximum concurrent Judge tasks per event."
						value={form.judgeMaxConcurrency}
						onChange={set("judgeMaxConcurrency")}
						min={1}
						max={1000}
						disabled={!editable}
					/>
					<NumberField
						label="Default Timeout"
						caption="Judge default timeout (seconds)."
						value={form.judgeDefaultTimeoutSecs}
						onChange={set("judgeDefaultTimeoutSecs")}
						min={1}
						max={3600}
						disabled={!editable}
					/>
					<NumberField
						label="Retry Interval"
						caption="Judge retry interval (seconds)."
						value={form.judgeRetryIntervalSecs}
						onChange={set("judgeRetryIntervalSecs")}
						min={1}
						max={3600}
						disabled={!editable}
					/>
				</Section>

				<Section title="Lifecycle">
					<NumberField
						label="Archive Retention"
						caption="Hours to retain after Finished before auto-archive."
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

function formatDuration(seconds: number): string {
	if (seconds < 0) return "invalid";
	const h = Math.floor(seconds / 3600);
	const m = Math.floor((seconds % 3600) / 60);
	const s = seconds % 60;
	if (h > 0) return `${h}h ${m}m ${s}s`;
	if (m > 0) return `${m}m ${s}s`;
	return `${s}s`;
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