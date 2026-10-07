import {
	Box,
	Button,
	ButtonGroup,
	FormControl,
	Spinner,
	TextInput,
	useConfirm,
} from "@primer/react";
import { InlineMessage } from "@primer/react/experimental";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";

import { adminApi } from "@/api";
import type { AwdPrecheckRun, AwdScoreRow } from "@floatctf/sdk";
import { useMsgBanner } from "@/components";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/awd/$id/ops")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

type PrecheckEntry = { component: string; error?: string; note?: string };
type PrecheckReport = { errors: PrecheckEntry[]; notes: PrecheckEntry[] };

/** 解析预检落库的 error_msg JSON；非法内容按单条错误展示，避免吞掉原因。 */
function parsePrecheckReport(raw?: string | null): PrecheckReport {
	if (!raw) return { errors: [], notes: [] };
	try {
		const parsed = JSON.parse(raw) as {
			errors?: PrecheckEntry[];
			notes?: PrecheckEntry[];
		};
		return { errors: parsed.errors ?? [], notes: parsed.notes ?? [] };
	} catch {
		return { errors: [{ component: "precheck", error: raw }], notes: [] };
	}
}

function formatPrecheckTime(value?: string | null) {
	if (!value) return "-";
	const parsed = new Date(value);
	return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString();
}

function RouteComponent() {
	const { id } = Route.useParams();
	const confirmDialog = useConfirm();
	const banner = useMsgBanner({});
	const qc = useQueryClient();

	const statusQuery = useQuery({
		queryKey: ["admin-awd-status", id],
		queryFn: () => adminApi.awd.getStatus(id),
	});

	const scores = useQuery({
		queryKey: ["admin-awd-scores", id],
		queryFn: () => adminApi.awd.scores(id),
	});

	// 预检失败原因（errors/notes）此前只落在数据库里，运维页必须能直接看到。
	const prechecks = useQuery({
		queryKey: ["admin-awd-prechecks", id],
		queryFn: () => adminApi.awd.prechecks(id),
	});

	const awd = statusQuery.data?.data ?? null;
	const latestPrecheck: AwdPrecheckRun | null =
		prechecks.data?.data?.[0] ?? null;
	const precheckReport = parsePrecheckReport(latestPrecheck?.error_msg);

	const onOk = (label: string) => () => {
		banner.showBanner("success", `${label}成功`);
		qc.invalidateQueries({ queryKey: ["admin-awd-scores", id] });
		qc.invalidateQueries({ queryKey: ["admin-awd-status", id] });
		qc.invalidateQueries({ queryKey: ["admin-awd-prechecks", id] });
		qc.invalidateQueries({ queryKey: ["event", id] });
	};

	const deploy = useMutation({
		mutationFn: () => adminApi.awd.deploy(id),
		onSuccess: onOk("部署"),
		onError: (e) => banner.showErrorBanner(e),
	});
	const precheck = useMutation({
		mutationFn: () => adminApi.awd.precheck(id),
		onSuccess: onOk("预检"),
		onError: (e) => banner.showErrorBanner(e),
	});
	const start = useMutation({
		mutationFn: () => adminApi.awd.start(id),
		onSuccess: onOk("开赛"),
		onError: (e) => banner.showErrorBanner(e),
	});
	const pause = useMutation({
		mutationFn: () => adminApi.awd.pause(id),
		onSuccess: onOk("暂停"),
		onError: (e) => banner.showErrorBanner(e),
	});
	const resume = useMutation({
		mutationFn: () => adminApi.awd.resume(id),
		onSuccess: onOk("恢复"),
		onError: (e) => banner.showErrorBanner(e),
	});
	const finish = useMutation({
		mutationFn: () => adminApi.awd.finish(id),
		onSuccess: onOk("结束比赛"),
		onError: (e) => banner.showErrorBanner(e),
	});
	const archive = useMutation({
		mutationFn: () => adminApi.awd.archive(id),
		onSuccess: onOk("归档"),
		onError: (e) => banner.showErrorBanner(e),
	});
	const rotate = useMutation({
		mutationFn: () => adminApi.awd.rotateTokens(id),
		onSuccess: onOk("令牌轮换"),
		onError: (e) => banner.showErrorBanner(e),
	});

	// Score Adjust
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
			banner.showBanner("success", "分数已调整");
			qc.invalidateQueries({ queryKey: ["admin-awd-scores", id] });
			setAdjDelta("0");
			setAdjReason("");
		},
		onError: (e) => banner.showErrorBanner(e),
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
	const status = awd?.status ?? "unknown";
	const isFinalSettlement = awd?.final_settlement ?? false;
	const isFinished = status === "finished" || status === "archived";

	return (
		<div className="flex flex-col gap-4 m-2" style={{ maxWidth: 920 }}>
			<banner.BannerComponent />

			{/* Lifecycle Actions */}
			<section>
				<h4 className="font-bold mb-2">生命周期</h4>

				{/* Contextual state banner */}
				{isFinalSettlement && (
					<InlineMessage variant="warning" className="mb-2">
						<strong>终局结算</strong> —— 最终评测正在结算，赛事操作已关闭；
						待所有评测任务结束且分数结算完成后，赛事将变为「已结束」。
					</InlineMessage>
				)}
				{status === "network_error" && (
					<InlineMessage variant="critical" className="mb-2">
						<strong>网络异常</strong> —— 平台基础设施故障，恢复后请点击「恢复」。
					</InlineMessage>
				)}
				{status === "paused" && (
					<InlineMessage variant="warning" className="mb-2">
						<strong>已暂停</strong> —— 比赛已冻结，点击「恢复」继续。
					</InlineMessage>
				)}
				{isFinished && (
					<InlineMessage variant="success" className="mb-2">
						<strong>{status === "archived" ? "已归档" : "已结束"}</strong> —
						Competition ended.
						{status === "finished" && " 可随时归档。"}
					</InlineMessage>
				)}

				<ButtonGroup>
					{/* Pre-Running: Deploy, Precheck, Start */}
					{["draft", "configuring", "deploy_failed"].includes(status) &&
						!isFinalSettlement && (
							<Button
								variant="primary"
								disabled={pending}
								onClick={() => deploy.mutate()}
							>
								部署
							</Button>
						)}
					{["deployed", "verification_failed", "configuring", "draft"].includes(
						status,
					) &&
						!isFinalSettlement && (
							<Button disabled={pending} onClick={() => precheck.mutate()}>
								预检
							</Button>
						)}
					{["verified", "start_blocked"].includes(status) &&
						!isFinalSettlement && (
							<Button
								variant="primary"
								disabled={pending}
								onClick={() => start.mutate()}
							>
								开赛
							</Button>
						)}

					{/* Running (normal): Pause only — no manual Finish */}
					{status === "running" && !isFinalSettlement && (
						<Button disabled={pending} onClick={() => pause.mutate()}>
							暂停
						</Button>
					)}

					{/* Paused: Resume */}
					{status === "paused" && !isFinalSettlement && (
						<Button
							variant="primary"
							disabled={pending}
							onClick={() => resume.mutate()}
						>
							恢复
						</Button>
					)}

					{/* NetworkError: Resume */}
					{status === "network_error" && !isFinalSettlement && (
						<Button
							variant="primary"
							disabled={pending}
							onClick={() => resume.mutate()}
						>
							恢复
						</Button>
					)}

					{/* Finished: Archive */}
					{status === "finished" && (
						<Button
							variant="danger"
							disabled={pending}
							onClick={async () => {
								const ok = await confirmDialog({
									title: "归档赛事？",
									content:
										"Archived events cannot be modified. GameBox containers may be cleaned up.",
									confirmButtonType: "danger",
								});
								if (ok) archive.mutate();
							}}
						>
							归档
						</Button>
					)}
				</ButtonGroup>

				{/* Rotate Tokens — always available when configured */}
				{awd && !isFinished && (
					<div className="mt-2">
						<Button
							variant="danger"
							disabled={pending}
							onClick={async () => {
								const ok = await confirmDialog({
									title: "轮换内部令牌？",
									content:
										"Will increment key_version, re-encrypt, and rebuild FlagServer/JudgeServer containers.",
									confirmButtonType: "danger",
								});
								if (ok) rotate.mutate();
							}}
						>
							轮换内部令牌
						</Button>
					</div>
				)}

				{pending && (
					<span className="ml-2">
						<Spinner size="small" />
					</span>
				)}
			</section>

			{/* Precheck Report：失败原因（errors/notes） */}
			{latestPrecheck && (
				<section>
					<h4 className="font-bold mb-2">预检报告</h4>
					<InlineMessage
						variant={
							precheckReport.errors.length === 0 ? "success" : "critical"
						}
						className="mb-2"
					>
						<strong>Precheck {latestPrecheck.status}</strong> —{" "}
						{formatPrecheckTime(
							latestPrecheck.completed_at ?? latestPrecheck.started_at,
						)}
						{latestPrecheck.revision !== null &&
							latestPrecheck.revision !== undefined &&
							` · revision ${latestPrecheck.revision}`}
					</InlineMessage>
					{precheckReport.errors.length > 0 && (
						<ul className="mb-2 flex list-disc flex-col gap-1 pl-6 text-sm">
							{precheckReport.errors.map((entry, index) => (
								<li key={`${entry.component}-${index}`}>
									<code>{entry.component}</code>: {entry.error}
								</li>
							))}
						</ul>
					)}
					{precheckReport.notes.length > 0 && (
						<details className="text-sm">
							<summary className="cursor-pointer">
								Details ({precheckReport.notes.length})
							</summary>
							<ul className="mt-1 flex list-disc flex-col gap-1 pl-6 text-gray-600">
								{precheckReport.notes.map((entry, index) => (
									<li key={`${entry.component}-${index}`}>
										<code>{entry.component}</code>: {entry.note}
									</li>
								))}
							</ul>
						</details>
					)}
				</section>
			)}

			{/* Score Adjust */}
			{!isFinished && (
				<section>
					<h4 className="font-bold mb-2">分数调整（记审计）</h4>
					<Box
						sx={{
							p: 3,
							border: "1px solid",
							borderColor: "border.default",
							borderRadius: 2,
						}}
					>
						<div className="flex items-center gap-2 flex-wrap">
							<FormControl disabled={adjust.isPending}>
								<FormControl.Label>队伍</FormControl.Label>
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
							</FormControl>
							<FormControl disabled={adjust.isPending}>
								<FormControl.Label>分数增减</FormControl.Label>
								<TextInput
									aria-label="delta"
									placeholder="e.g. 100 or -50"
									value={adjDelta}
									onChange={(e) => setAdjDelta(e.target.value)}
									disabled={adjust.isPending}
									style={{ width: 160 }}
								/>
							</FormControl>
							<FormControl disabled={adjust.isPending}>
								<FormControl.Label>原因</FormControl.Label>
								<TextInput
									aria-label="reason"
									placeholder="reason"
									value={adjReason}
									onChange={(e) => setAdjReason(e.target.value)}
									disabled={adjust.isPending}
									style={{ width: 240 }}
								/>
							</FormControl>
							<Button
								disabled={!adjTeam || adjust.isPending || pending}
								onClick={() => adjust.mutate()}
							>
								应用
							</Button>
						</div>
					</Box>
				</section>
			)}

			{/* Scoreboard */}
			<section>
				<h4 className="font-bold mb-2">排行榜</h4>
				{scores.isLoading ? <Spinner /> : <AdminScoreboard rows={rows} />}
			</section>
		</div>
	);
}

function AdminScoreboard({ rows }: { rows: AwdScoreRow[] }) {
	return (
		<table className="w-full text-sm">
			<thead>
				<tr>
					<th align="left">#</th>
					<th align="left">队伍</th>
					<th align="right">攻击分</th>
					<th align="right">防守分</th>
					<th align="right">总分</th>
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
	);
}
