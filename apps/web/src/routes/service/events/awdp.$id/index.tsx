import {
	Box,
	Button,
	FormControl,
	Heading,
	Label,
	Spinner,
	TextInput,
	Tooltip,
} from "@primer/react";
import { InlineMessage } from "@primer/react/experimental";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import dayjs from "dayjs";
import { useMemo, useRef, useState } from "react";

import { awdpPlayerApi, type AwdpGameBox } from "@/api/awdp";
import { useMsgBanner } from "@/components";
import { useAwdpEventStream } from "@/hooks/useAwdpEventStream";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awdp/$id/")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

const PHASE_LABEL: Record<string, { text: string; variant: "attention" | "success" | "severe" | "done" }> = {
	pending: { text: "Pending", variant: "attention" },
	break: { text: "Break", variant: "attention" },
	fix: { text: "Fix", variant: "success" },
	ended: { text: "Ended", variant: "done" },
};

const EVAL_LABEL: Record<string, string> = {
	pending: "评估中…",
	running: "评估中…",
	no_patch: "NO_PATCH",
	service_down: "SERVICE_DOWN",
	functional_broken: "BROKEN",
	vulnerable: "VULNERABLE",
	patched: "PATCHED",
	platform_error: "PLATFORM_ERROR",
};

function fmtTime(iso?: string | null) {
	if (!iso) return "-";
	return dayjs.utc(iso).local().format("MM-DD HH:mm:ss");
}

function countdown(target?: string | null) {
	if (!target) return "-";
	const diff = dayjs(target).diff(dayjs());
	if (diff <= 0) return "0s";
	const s = Math.floor(diff / 1000);
	const h = Math.floor(s / 3600);
	const m = Math.floor((s % 3600) / 60);
	const sec = s % 60;
	return h > 0 ? `${h}h ${m}m` : m > 0 ? `${m}m ${sec}s` : `${sec}s`;
}

function GameBoxCard({
	eventId,
	gb,
	phase,
	refetch,
	banner,
}: {
	eventId: string;
	gb: AwdpGameBox;
	phase: string;
	refetch: () => void;
	banner: ReturnType<typeof useMsgBanner>;
}) {
	const queryClient = useQueryClient();
	const [flag, setFlag] = useState("");
	const fileRef = useRef<HTMLInputElement>(null);
	const [checking, setChecking] = useState(false);
	const [manualResult, setManualResult] = useState<{ health: boolean; judge: boolean; detail: string } | null>(null);

	const invalidate = () => {
		queryClient.invalidateQueries({ queryKey: ["awdp-overview", eventId] });
		refetch();
	};

	const start = useMutation({
		mutationFn: () => awdpPlayerApi.startInstance(eventId, gb.id),
		onSuccess: () => {
			banner.showBanner("success", "实例已启动");
			invalidate();
		},
		onError: (e) => banner.showErrorBanner(e),
	});
	const stop = useMutation({
		mutationFn: () => awdpPlayerApi.stopInstance(eventId, gb.id),
		onSuccess: () => {
			banner.showBanner("info", "实例已停止");
			invalidate();
		},
		onError: (e) => banner.showErrorBanner(e),
	});
	const reset = useMutation({
		mutationFn: () => awdpPlayerApi.resetInstance(eventId, gb.id),
		onSuccess: () => {
			banner.showBanner("info", "已重置为初始状态");
			invalidate();
		},
		onError: (e) => banner.showErrorBanner(e),
	});
	const submitBreak = useMutation({
		mutationFn: (f: string) => awdpPlayerApi.submitBreak(eventId, gb.id, f),
		onSuccess: (res) => {
			const d = res.data.data;
			if (!d) return;
			if (d.accepted) {
				banner.showBanner("success", d.scored ? `+Break 得分！` : d.already_broken ? "已打穿（本盒不再计分）" : "Flag 正确");
			} else {
				banner.showBanner("critical", "Flag 错误");
			}
			setFlag("");
			invalidate();
		},
		onError: (e) => banner.showErrorBanner(e),
	});
	const uploadPatch = useMutation({
		mutationFn: (file: File) => awdpPlayerApi.uploadPatch(eventId, gb.id, file),
		onSuccess: (res) => {
			const status = res.data.data?.status ?? "failed";
			banner.showBanner(status === "applied" ? "success" : "critical", status === "applied" ? "Patch 已应用并重启" : "Patch 应用失败（exit != 0）");
			invalidate();
		},
		onError: (e) => banner.showErrorBanner(e),
	});
	const testCheck = useMutation({
		mutationFn: () => awdpPlayerApi.testCheck(eventId, gb.id),
		onSuccess: (res) => {
			const d = res.data.data;
			if (!d) return;
			setChecking(false);
			setManualResult({ health: d.healthcheck_ok, judge: d.judge_ok, detail: `${d.healthcheck_detail.join("; ")} | ${d.judge_detail}` });
			banner.showBanner(d.healthcheck_ok && d.judge_ok ? "success" : "critical", d.healthcheck_ok && d.judge_ok ? "健康检查与功能检查均通过（不计分）" : "检查未通过");
		},
		onError: (e) => {
			setChecking(false);
			banner.showErrorBanner(e);
		},
	});

	const inst = gb.instance;
	return (
		<section className="rounded border p-3 mb-3">
			<div className="flex items-center gap-2 mb-2">
				<Heading as="h2" sx={{ flex: 1 }}>
					{gb.name}
				</Heading>
				<Label variant={gb.broken ? "success" : "secondary"}>{gb.broken ? "Broken" : "Unbroken"}</Label>
				<Label variant={inst?.runtime_state === "running" ? "success" : "secondary"}>
					{inst ? inst.runtime_state : "未启动"}
				</Label>
			</div>
			<p className="text-sm text-gray-500 mb-2">
				{gb.category} · 暴露端口：{gb.exposed.map(([p, port]) => `${p}:${port}`).join(" / ")}
			</p>

			{/* 启动 / 停止 / 重置 */}
			<div className="flex gap-2 mb-3">
				<Button size="small" variant="primary" disabled={inst?.runtime_state === "running" || start.isPending} onClick={() => start.mutate()}>
					Start GameBox
				</Button>
				<Button size="small" disabled={inst?.runtime_state !== "running" || stop.isPending} onClick={() => stop.mutate()}>
					Stop
				</Button>
				<Button size="small" disabled={!inst || reset.isPending} onClick={() => reset.mutate()}>
					Reset
				</Button>
			</div>

			{/* 端点 */}
			{inst && inst.endpoints.length > 0 && (
				<div className="mb-3">
					<p className="text-xs font-bold text-gray-500 mb-1">Target Endpoints</p>
					<div className="flex flex-col gap-1">
						{inst.endpoints.map((ep) =>
							ep.protocol === "http" ? (
								<div key={`${ep.protocol}-${ep.container_port}`} className="flex items-center gap-2 text-sm font-mono">
									<span className="text-gray-400">Web</span>
									<a href={`http://${ep.public_host}:${ep.public_port}`} target="_blank" rel="noreferrer">
										http://{ep.public_host}:{ep.public_port}
									</a>
									<Button size="small" onClick={() => navigator.clipboard?.writeText(`http://${ep.public_host}:${ep.public_port}`)}>
										Copy
									</Button>
								</div>
							) : (
								<div key={`${ep.protocol}-${ep.container_port}`} className="flex items-center gap-2 text-sm font-mono">
									<span className="text-gray-400">TCP</span>
									<span>
										{ep.public_host}:{ep.public_port} · nc {ep.public_host} {ep.public_port}
									</span>
									<Button size="small" onClick={() => navigator.clipboard?.writeText(`nc ${ep.public_host} ${ep.public_port}`)}>
										Copy
									</Button>
								</div>
							),
						)}
					</div>
				</div>
			)}

			{/* Break：flag 提交 */}
			{phase === "break" && (
				<div className="mb-2">
					{gb.broken ? (
						<p className="text-sm text-green-600">已打穿本 GameBox（Break 得分已计入一次）</p>
					) : (
						<div className="flex gap-2 items-center">
							<FormControl>
								<FormControl.Label visuallyHidden>Flag</FormControl.Label>
								<TextInput
									value={flag}
									onChange={(e) => setFlag(e.target.value)}
									placeholder="flag{...}"
									monospace
									onKeyDown={(e) => e.key === "Enter" && flag && submitBreak.mutate(flag)}
								/>
							</FormControl>
							<Button variant="primary" disabled={!flag || submitBreak.isPending} onClick={() => submitBreak.mutate(flag)}>
								Submit Flag
							</Button>
						</div>
					)}
				</div>
			)}

			{/* Fix：源码 + patch + test-check */}
			{phase === "fix" && (
				<div className="flex flex-col gap-2">
					<div className="flex items-center gap-2">
						<Button
							size="small"
							onClick={() => {
								awdpPlayerApi
									.sourceUrl(eventId, gb.id)
									.then((res) => {
										window.open(res.data.data, "_blank");
									})
									.catch((e) => banner.showErrorBanner(e));
							}}
						>
							Download Source
						</Button>
						<span className="text-xs text-gray-500">
							{gb.source_code_dir ? `Source path: ${gb.source_code_dir}` : "无源码目录"}
						</span>
					</div>
					<div className="flex items-center gap-2">
						<input
							ref={fileRef}
							type="file"
							accept=".sh"
							className="hidden"
							onChange={(e) => {
								const f = e.target.files?.[0];
								if (f) uploadPatch.mutate(f);
								e.target.value = "";
							}}
						/>
						<Button size="small" onClick={() => fileRef.current?.click()} disabled={uploadPatch.isPending}>
							{uploadPatch.isPending ? "Applying…" : "Upload patch.sh"}
						</Button>
						<Button size="small" onClick={() => { setChecking(true); testCheck.mutate(); }} disabled={checking || testCheck.isPending}>
							{checking ? "Checking…" : "Test Check"}
						</Button>
					</div>
					{manualResult && (
						<p className={`text-xs ${manualResult.health && manualResult.judge ? "text-green-600" : "text-red-600"}`}>
							health: {manualResult.health ? "PASS" : "FAIL"} · judge: {manualResult.judge ? "PASS" : "FAIL"} — {manualResult.detail}
						</p>
					)}
				</div>
			)}
		</section>
	);
}

function RouteComponent() {
	const { id } = Route.useParams();
	const { connected } = useAwdpEventStream({ eventId: id });
	const banner = useMsgBanner();
	const { data, isLoading, isError, refetch } = useQuery({
		queryKey: ["awdp-overview", id],
		queryFn: () => awdpPlayerApi.overview(id),
	});
	const { data: roundsData } = useQuery({
		queryKey: ["awdp-rounds", id],
		queryFn: () => awdpPlayerApi.rounds(id),
		enabled: !!data?.data?.data,
	});
	const { data: evalsData } = useQuery({
		queryKey: ["awdp-evals", id],
		queryFn: () => awdpPlayerApi.evaluations(id),
		enabled: !!data?.data?.data,
	});

	const ov = data?.data?.data;
	const phase = ov?.phase ?? "pending";
	const phaseMeta = PHASE_LABEL[phase] ?? PHASE_LABEL.pending;
	const deadline =
		phase === "break" ? ov?.break_ends_at : phase === "fix" ? ov?.fix_ends_at : ov?.next_action_at;

	const roundRows = useMemo(() => {
		if (!roundsData?.data?.data || !evalsData?.data?.data) return [];
		const evals = evalsData.data.data;
		return roundsData.data.data.map((r) => {
			const mine = evals.filter((e) => e.round_sequence === r.sequence && e.kind === "official");
			return {
				round: r,
				results: mine.map((e) => EVAL_LABEL[e.status] ?? e.status),
				score: mine.filter((e) => e.status === "patched").length > 0 ? "✓" : "",
			};
		});
	}, [roundsData, evalsData]);

	if (isLoading) {
		return (
			<div className="p-8 flex justify-center">
				<Spinner size="large" />
			</div>
		);
	}
	if (isError || !ov) {
		return (
			<div className="p-4">
				<InlineMessage variant="critical">Failed to load AWDP event.</InlineMessage>
			</div>
		);
	}

	return (
		<div className="p-3">
			<InlineMessage variant="success">
				<div>
					AWD Plus：{phaseMeta.text}
					{phase !== "ended" && phase !== "pending" && (
						<span>
							{" "}
							· 剩余 {countdown(deadline)} {connected ? "· 实时" : "· 轮询"}
						</span>
					)}
				</div>
			</InlineMessage>

			{/* 顶部状态 */}
			<section className="rounded border p-3 mt-3 mb-3 flex flex-wrap items-center gap-4">
				<div>
					<p className="text-xs text-gray-500">Phase</p>
					<Label variant={phaseMeta.variant}>{phaseMeta.text}</Label>
				</div>
				<div>
					<p className="text-xs text-gray-500">Current Score</p>
					<p className="text-xl font-bold">{ov.my_score}</p>
				</div>
				<div>
					<p className="text-xs text-gray-500">Break Score / GameBox</p>
					<p className="text-lg font-semibold">+{ov.break_score}</p>
				</div>
				<div>
					<p className="text-xs text-gray-500">Fix Score / Turn</p>
					<p className="text-lg font-semibold">+{ov.fix_round_score}</p>
				</div>
				{phase === "fix" && (
					<div>
						<p className="text-xs text-gray-500">Round</p>
						<p className="text-lg font-semibold">
							{Math.min(ov.current_round + 1, ov.total_rounds)} / {ov.total_rounds}
						</p>
					</div>
				)}
				{phase === "fix" && (
					<div>
						<p className="text-xs text-gray-500">Next Check In</p>
						<p className="text-lg font-semibold">{countdown(ov.next_action_at)}</p>
					</div>
				)}
				<div>
					<p className="text-xs text-gray-500">Break 至</p>
					<p className="text-sm font-medium">{fmtTime(ov.break_ends_at)}</p>
				</div>
				<div>
					<p className="text-xs text-gray-500">Fix 至</p>
					<p className="text-sm font-medium">{fmtTime(ov.fix_ends_at)}</p>
				</div>
			</section>

			{/* GameBox 卡片 */}
			<div>
				{ov.gameboxes.length === 0 && (
					<InlineMessage variant="warning">本赛事尚未配置 GameBox。</InlineMessage>
				)}
				{ov.gameboxes.map((gb) => (
					<GameBoxCard key={gb.id} eventId={id} gb={gb} phase={phase} refetch={refetch} banner={banner} />
				))}
			</div>

			{/* Fix 回合历史 */}
			{phase === "fix" && roundRows.length > 0 && (
				<section className="rounded border p-3 mt-2">
					<Heading as="h2" sx={{ mb: 2 }}>
						Official Round History
					</Heading>
					<table className="w-full text-sm">
						<thead>
							<tr className="text-left text-gray-500">
								<th className="py-1">Round</th>
								<th className="py-1">窗口</th>
								<th className="py-1">结果</th>
								<th className="py-1">状态</th>
							</tr>
						</thead>
						<tbody>
							{roundRows.map(({ round, results, score }) => (
								<tr key={round.id} className="border-t">
									<td className="py-1 font-medium">R{round.sequence}</td>
									<td className="py-1 text-gray-500">
										{fmtTime(round.starts_at)} ~ {fmtTime(round.cutoff_at)}
									</td>
									<td className="py-1">
										{results.length ? results.join(" / ") : "-"}
									</td>
									<td className="py-1">{score || round.status}</td>
								</tr>
							))}
						</tbody>
					</table>
				</section>
			)}
		</div>
	);
}
