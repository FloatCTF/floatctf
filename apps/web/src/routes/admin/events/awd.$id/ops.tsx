import {
	Button,
	ButtonGroup,
	Spinner,
	TextInput,
	useConfirm,
} from "@primer/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";

import { adminApi } from "@/api";
import { useMsgBanner } from "@/components";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/awd/$id/ops")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const confirmDialog = useConfirm();
	const banner = useMsgBanner({});
	const qc = useQueryClient();

	const scores = useQuery({
		queryKey: ["admin-awd-scores", id],
		queryFn: () => adminApi.awd.scores(id),
	});

	const onOk = (label: string) => () => {
		banner.showBanner("success", `${label} ok`);
		qc.invalidateQueries({ queryKey: ["admin-awd-scores", id] });
		qc.invalidateQueries({ queryKey: ["admin-awd-status", id] });
		qc.invalidateQueries({ queryKey: ["event", id] });
	};

	const deploy = useMutation({
		mutationFn: () => adminApi.awd.deploy(id),
		onSuccess: onOk("Deploy"),
		onError: banner.showErrorBanner,
	});
	const precheck = useMutation({
		mutationFn: () => adminApi.awd.precheck(id),
		onSuccess: onOk("Precheck"),
		onError: banner.showErrorBanner,
	});
	const start = useMutation({
		mutationFn: () => adminApi.awd.start(id),
		onSuccess: onOk("Start"),
		onError: banner.showErrorBanner,
	});
	const pause = useMutation({
		mutationFn: () => adminApi.awd.pause(id),
		onSuccess: onOk("Pause"),
		onError: banner.showErrorBanner,
	});
	const resume = useMutation({
		mutationFn: () => adminApi.awd.resume(id),
		onSuccess: onOk("Resume"),
		onError: banner.showErrorBanner,
	});
	const finish = useMutation({
		mutationFn: () => adminApi.awd.finish(id),
		onSuccess: onOk("Finish"),
		onError: banner.showErrorBanner,
	});
	const archive = useMutation({
		mutationFn: () => adminApi.awd.archive(id),
		onSuccess: onOk("Archive"),
		onError: banner.showErrorBanner,
	});
	const rotate = useMutation({
		mutationFn: () => adminApi.awd.rotateTokens(id),
		onSuccess: onOk("Token Rotated"),
		onError: banner.showErrorBanner,
	});

	// Score Adjust（P5-11 审计）
	const [adjTeam, setAdjTeam] = useState("");
	const [adjDelta, setAdjDelta] = useState("0");
	const [adjReason, setAdjReason] = useState("");
	const adjust = useMutation({
		mutationFn: () =>
			adminApi.awd.adjustScore(id, {
				team_id: adjTeam,
				delta: Number.parseInt(adjDelta, 10) || 0,
				reason: adjReason.trim() || "manual adjustment",
			}),
		onSuccess: () => {
			banner.showBanner("success", "Score adjusted");
			qc.invalidateQueries({ queryKey: ["admin-awd-scores", id] });
			setAdjDelta("0");
			setAdjReason("");
		},
		onError: banner.showErrorBanner,
	});

	const pending =
		deploy.isPending ||
		precheck.isPending ||
		start.isPending ||
		pause.isPending ||
		resume.isPending ||
		finish.isPending ||
		archive.isPending ||
		rotate.isPending ||
		adjust.isPending;

	const rows = scores.data?.data ?? [];

	return (
		<div className="flex flex-col gap-4 m-2">
			<banner.BannerComponent />
			<section>
				<h4 className="font-bold mb-2">Lifecycle</h4>
				<ButtonGroup>
					<Button disabled={pending} onClick={() => deploy.mutate()}>
						Deploy
					</Button>
					<Button disabled={pending} onClick={() => precheck.mutate()}>
						Precheck
					</Button>
					<Button
						variant="primary"
						disabled={pending}
						onClick={() => start.mutate()}
					>
						Start
					</Button>
					<Button disabled={pending} onClick={() => pause.mutate()}>
						Pause
					</Button>
					<Button disabled={pending} onClick={() => resume.mutate()}>
						Resume
					</Button>
					<Button disabled={pending} onClick={() => finish.mutate()}>
						Finish
					</Button>
					<Button
						variant="danger"
						disabled={pending}
						onClick={() => archive.mutate()}
					>
						Archive
					</Button>
					<Button
						variant="danger"
						disabled={pending}
						onClick={async () => {
							const ok = await confirmDialog({
								title: "确认轮换内部 Token？",
								content:
									"将 key_version+1、重新加密并重建 FlagServer/JudgeServer 容器。",
								confirmButtonType: "danger",
							});
							if (ok) rotate.mutate();
						}}
					>
						Rotate Tokens
					</Button>
				</ButtonGroup>
				{pending && (
					<span className="ml-2">
						<Spinner size="small" />
					</span>
				)}
			</section>

			<section>
				<h4 className="font-bold mb-2">Score Adjust (audited)</h4>
				<div className="flex items-center gap-2 flex-wrap">
					<select
						className="border rounded px-2 py-1 text-sm"
						value={adjTeam}
						onChange={(e) => setAdjTeam(e.target.value)}
						disabled={adjust.isPending}
					>
						<option value="">Select team…</option>
						{rows.map((r) => (
							<option key={r.team_id} value={r.team_id}>
								{r.team_name}
							</option>
						))}
					</select>
					<TextInput
						aria-label="delta"
						placeholder="delta (e.g. 100 / -50)"
						value={adjDelta}
						onChange={(e) => setAdjDelta(e.target.value)}
						disabled={adjust.isPending}
						style={{ width: 180 }}
					/>
					<TextInput
						aria-label="reason"
						placeholder="reason"
						value={adjReason}
						onChange={(e) => setAdjReason(e.target.value)}
						disabled={adjust.isPending}
						style={{ width: 240 }}
					/>
					<Button
						disabled={!adjTeam || adjust.isPending || pending}
						onClick={() => adjust.mutate()}
					>
						Apply
					</Button>
				</div>
			</section>

			<section>
				<h4 className="font-bold mb-2">Scoreboard</h4>
				{scores.isLoading ? (
					<Spinner />
				) : (
					<table className="w-full text-sm">
						<thead>
							<tr>
								<th align="left">#</th>
								<th align="left">Team</th>
								<th align="right">Attack</th>
								<th align="right">Defense</th>
								<th align="right">Total</th>
							</tr>
						</thead>
						<tbody>
							{rows.map((r) => (
								<tr key={r.team_id}>
									<td>{r.rank}</td>
									<td>{r.team_name}</td>
									<td align="right">{r.attack_score}</td>
									<td align="right">{r.defense_score}</td>
									<td align="right">
										<strong>{r.total_score}</strong>
									</td>
								</tr>
							))}
							{rows.length === 0 && (
								<tr>
									<td colSpan={5}>No scores yet.</td>
								</tr>
							)}
						</tbody>
					</table>
				)}
			</section>
		</div>
	);
}
