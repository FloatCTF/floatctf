import {
	Box,
	Label,
	Spinner,
} from "@primer/react";
import { InlineMessage } from "@primer/react/experimental";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import dayjs from "dayjs";

import { adminApi } from "@/api";
import type { AwdEventStatus } from "@/api/awd";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/awd/$id/")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

function formatDate(iso?: string | null) {
	if (!iso) return "-";
	return dayjs.utc(iso).local().format("YYYY-MM-DD HH:mm:ss");
}

function RouteComponent() {
	const { id } = Route.useParams();
	const eventQuery = useQuery({
		queryKey: ["event", id],
		queryFn: () => adminApi.events.get(id),
	});
	const statusQuery = useQuery({
		queryKey: ["admin-awd-status", id],
		queryFn: () => adminApi.awd.getStatus(id),
	});

	const event = eventQuery.data?.data;
	const awd = statusQuery.data?.data ?? null;

	if (eventQuery.isLoading || statusQuery.isLoading) {
		return <Spinner size="large" />;
	}
	if (!event) {
		return <InlineMessage variant="warning">Event not found.</InlineMessage>;
	}
	if (!awd) {
		return <InlineMessage variant="warning">AWD not configured. Go to Configure tab first.</InlineMessage>;
	}

	return <Overview event={event} awd={awd} />;
}

function Overview({ event, awd }: { event: { title: string; start_time?: string; end_time?: string }; awd: AwdEventStatus }) {
	const statusLabel = statusVariant(awd.status);
	const phaseLabel = awd.phase ? phaseVariant(awd.phase) : null;

	return (
		<div className="mt-3" style={{ maxWidth: 920 }}>
			<div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
				{/* Status */}
				<StatCard title="AWD Status">
					<Label variant={statusLabel.variant}>{statusLabel.label}</Label>
				</StatCard>

				{/* Phase */}
				<StatCard title="Phase">
					{phaseLabel ? (
						<Label variant={phaseLabel.variant}>{phaseLabel.label}</Label>
					) : (
						<span className="text-sm text-[var(--fgColor-muted)]">-</span>
					)}
				</StatCard>

				{/* Round */}
				<StatCard title="Total Rounds">
					<span className="text-sm font-mono">
						{awd.round_count != null ? `${awd.round_count}` : "-"}
					</span>
				</StatCard>

				{/* Round Duration */}
				<StatCard title="Round Duration">
					<span className="text-sm font-mono">
						{awd.round_duration_secs}s
					</span>
				</StatCard>

				{/* Initial Score */}
				<StatCard title="Initial Score">
					<span className="text-sm font-mono">{awd.initial_score}</span>
				</StatCard>

				{/* Event Start */}
				<StatCard title="Event Start">
					<span className="text-sm">{formatDate(event.start_time)}</span>
				</StatCard>

				{/* Event End */}
				<StatCard title="Event End">
					<span className="text-sm">{formatDate(event.end_time)}</span>
				</StatCard>

				{/* Started At */}
				<StatCard title="Competition Started">
					<span className="text-sm">{formatDate(awd.started_at)}</span>
				</StatCard>

				{/* SSE connection state */}
				<StatCard title="Realtime">
					<span className="text-sm text-[var(--fgColor-muted)]">See top bar</span>
				</StatCard>
			</div>

			{/* NetworkError banner */}
			{awd.status === "network_error" && (
				<div className="mt-3">
					<InlineMessage variant="critical">
						<strong>Network Error</strong> — Platform infrastructure failure detected.
						Competition is paused. Administrator must Resume after recovery.
					</InlineMessage>
				</div>
			)}

			{/* Final Settlement banner */}
			{awd.final_settlement && (
				<div className="mt-3">
					<InlineMessage variant="warning">
						<strong>Final Settlement</strong> — Final Judge settlement is in progress.
						Competition actions are closed. The event will become Finished when all
						final Judge tasks are terminal and scoring is settled.
					</InlineMessage>
				</div>
			)}

			{/* Paused banner */}
			{awd.status === "paused" && (
				<div className="mt-3">
					<InlineMessage variant="warning">
						<strong>Paused</strong> — Competition is administratively paused.
						Players cannot access GameBoxes, submit Flags, or Reset.
					</InlineMessage>
				</div>
			)}

			{/* Finished banner */}
			{(awd.status === "finished" || awd.status === "archived") && (
				<div className="mt-3">
					<InlineMessage variant="success">
						<strong>{awd.status === "archived" ? "Archived" : "Finished"}</strong> — Scoreboard is final.
						{awd.status === "finished" && " Archive when ready."}
					</InlineMessage>
				</div>
			)}
		</div>
	);
}

function StatCard({ title, children }: { title: string; children: React.ReactNode }) {
	return (
		<Box
			sx={{
				p: 3,
				border: "1px solid",
				borderColor: "border.default",
				borderRadius: 2,
			}}
		>
			<div className="text-xs font-semibold text-[var(--fgColor-muted)] uppercase tracking-wide mb-1">
				{title}
			</div>
			{children}
		</Box>
	);
}

function statusVariant(status: string): { label: string; variant: "success" | "danger" | "attention" | "accent" | "default" } {
	switch (status) {
		case "running":
			return { label: "Running", variant: "success" };
		case "paused":
			return { label: "Paused", variant: "attention" };
		case "network_error":
			return { label: "Network Error", variant: "danger" };
		case "finished":
			return { label: "Finished", variant: "default" };
		case "archived":
			return { label: "Archived", variant: "default" };
		case "draft":
			return { label: "Draft", variant: "default" };
		case "configuring":
			return { label: "Configuring", variant: "accent" };
		case "deploying":
			return { label: "Deploying", variant: "accent" };
		case "deployed":
			return { label: "Deployed", variant: "accent" };
		case "prechecking":
			return { label: "Prechecking", variant: "accent" };
		case "verified":
			return { label: "Verified", variant: "success" };
		case "start_blocked":
			return { label: "Start Blocked", variant: "attention" };
		case "deploy_failed":
			return { label: "Deploy Failed", variant: "danger" };
		case "verification_failed":
			return { label: "Verification Failed", variant: "danger" };
		default:
			return { label: status, variant: "default" };
	}
}

function phaseVariant(phase: string): { label: string; variant: "success" | "danger" | "attention" | "accent" | "default" } {
	switch (phase) {
		case "hardening":
			return { label: "Hardening", variant: "accent" };
		case "attack":
			return { label: "Attack", variant: "success" };
		case "pause":
			return { label: "Pause", variant: "attention" };
		default:
			return { label: phase, variant: "default" };
	}
}