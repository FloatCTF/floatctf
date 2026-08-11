import { Button, FormControl, TextInput } from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive } from "ahooks";
import dayjs from "dayjs";
import { useEffect } from "react";

import { awdpAdminApi } from "@/api/awdp";
import { useMsgBanner } from "@/components";

export const Route = createFileRoute("/admin/events/awdp/$id/configure")({
	component: RouteComponent,
});

const DEFAULTS = {
	break_duration_secs: 3600,
	fix_duration_secs: 3600,
	fix_round_interval_secs: 600,
	break_score: 1000,
	fix_round_score: 150,
};

function fmt(iso: string | null | undefined) {
	return iso ? dayjs.utc(iso).local().format("YYYY-MM-DD HH:mm:ss") : "-";
}

function RouteComponent() {
	const { id } = Route.useParams();
	const queryClient = useQueryClient();
	const banner = useMsgBanner();
	const { data, isLoading } = useQuery({
		queryKey: ["awdp-config", id],
		queryFn: () => awdpAdminApi.getConfig(id),
	});
	const cfg = data?.data?.data;

	const form = useReactive<Record<string, number>>({ ...DEFAULTS });
	useEffect(() => {
		if (cfg) {
			form.break_duration_secs = cfg.break_duration_secs;
			form.fix_duration_secs = cfg.fix_duration_secs;
			form.fix_round_interval_secs = cfg.fix_round_interval_secs;
			form.break_score = cfg.break_score;
			form.fix_round_score = cfg.fix_round_score;
		}
	}, [cfg]);

	const locked = cfg ? cfg.phase !== "pending" : true;

	const save = useMutation({
		mutationFn: () =>
			awdpAdminApi.updateConfig(id, {
				expected_updated_at: cfg!.updated_at,
				break_duration_secs: Number(form.break_duration_secs),
				fix_duration_secs: Number(form.fix_duration_secs),
				fix_round_interval_secs: Number(form.fix_round_interval_secs),
				break_score: Number(form.break_score),
				fix_round_score: Number(form.fix_round_score),
			}),
		onSuccess: () => {
			banner.showBanner("success", "配置已保存");
			queryClient.invalidateQueries({ queryKey: ["awdp-config", id] });
		},
		onError: (e) => banner.showErrorBanner(e),
	});

	if (isLoading) {
		return <div className="p-4">Loading…</div>;
	}

	return (
		<div className="p-3 max-w-xl">
			<div className="mb-3 flex gap-4 text-sm">
				<span>
					Phase: <b>{cfg?.phase}</b>
				</span>
				<span>
					回合数: <b>{cfg?.total_rounds}</b>
				</span>
				<span>配置代数: {cfg?.configuration_generation}</span>
			</div>
			<div className="mb-3 grid grid-cols-2 gap-3">
				<FormControl disabled={locked}>
					<FormControl.Label>Break Duration (secs)</FormControl.Label>
					<TextInput value={form.break_duration_secs} onChange={(e) => (form.break_duration_secs = Number(e.target.value))} />
				</FormControl>
				<FormControl disabled={locked}>
					<FormControl.Label>Fix Duration (secs)</FormControl.Label>
					<TextInput value={form.fix_duration_secs} onChange={(e) => (form.fix_duration_secs = Number(e.target.value))} />
				</FormControl>
				<FormControl disabled={locked}>
					<FormControl.Label>Turn Interval (secs)</FormControl.Label>
					<TextInput value={form.fix_round_interval_secs} onChange={(e) => (form.fix_round_interval_secs = Number(e.target.value))} />
				</FormControl>
				<FormControl disabled={locked}>
					<FormControl.Label>Break Score</FormControl.Label>
					<TextInput value={form.break_score} onChange={(e) => (form.break_score = Number(e.target.value))} />
				</FormControl>
				<FormControl disabled={locked}>
					<FormControl.Label>Fix Score / Turn</FormControl.Label>
					<TextInput value={form.fix_round_score} onChange={(e) => (form.fix_round_score = Number(e.target.value))} />
				</FormControl>
			</div>
			{locked && (
				<p className="text-sm text-yellow-700 mb-2">
					赛事已开始（{cfg?.phase}），配置已冻结。
				</p>
			)}
			<Button variant="primary" disabled={locked || save.isPending} onClick={() => save.mutate()}>
				{save.isPending ? "Saving…" : "Save Config"}
			</Button>

			<section className="rounded border p-3 mt-4 text-sm">
				<p className="font-bold mb-2">Timeline</p>
				<dl className="grid grid-cols-[8rem_1fr] gap-y-1">
					<dt>Started</dt>
					<dd>{fmt(cfg?.started_at)}</dd>
					<dt>Break 至</dt>
					<dd>{fmt(cfg?.break_ends_at)}</dd>
					<dt>Fix 开始</dt>
					<dd>{fmt(cfg?.fix_started_at)}</dd>
					<dt>Fix 至</dt>
					<dd>{fmt(cfg?.fix_ends_at)}</dd>
					<dt>Finished</dt>
					<dd>{fmt(cfg?.finished_at)}</dd>
					<dt>Next Action</dt>
					<dd>{fmt(cfg?.next_action_at)}</dd>
				</dl>
			</section>
		</div>
	);
}
