import { Button, Spinner } from "@primer/react";
import { InlineMessage } from "@primer/react/experimental";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useTitle } from "ahooks";
import { useMemo } from "react";

import { awdpRunApi } from "@/api/awdpRuns";
import { useMsgBanner } from "@/components";
import {
	AwdpWorkbench,
	type AwdpWorkbenchViewModel,
	buildAwdpHistory,
} from "@/components/awdp/AwdpWorkbench";
import { useAwdpRunStream } from "@/hooks/useAwdpRunStream";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/awdp/runs/$runId/")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

/**
 * AWDP Practice Run 工作台（§63/§66-68）。
 * 与赛事路由完全隔离：数据全部来自 run-scoped API（冻结契约 §C.2），
 * 三态渲染交给共享 <AwdpWorkbench>（VM 由本页组装）。
 */
function RouteComponent() {
	const { runId } = Route.useParams();
	useTitle("AWDP Training | FloatCTF");
	const navigate = useNavigate();
	const queryClient = useQueryClient();
	const banner = useMsgBanner({});
	// SSE 实时刷新 + 断连轮询兜底（不渲染 live/poll 指示器）。
	useAwdpRunStream({ runId });

	/** 练习「开始」：冻结 run → 回卷全新 Break + 启动实例（与 Challenge 练习 Launch 同效）。 */
	const startMutation = useMutation({
		mutationFn: () => awdpRunApi.startRun(runId),
		onSuccess: (res) => {
			if (res.data) {
				queryClient.setQueryData(["awdp-run", runId], res);
			}
			invalidate();
		},
		onError: (error) => {
			banner.showErrorBanner(error);
		},
	});

	/** 练习「End」：停止全部实例并恢复如初（回到「开始」态）。 */
	const endMutation = useMutation({
		mutationFn: () => awdpRunApi.endRun(runId),
		onSuccess: (res) => {
			if (res.data) {
				queryClient.setQueryData(["awdp-run", runId], res);
			}
			invalidate();
			banner.showBanner("success", "训练已结束，实例已停止，恢复初始状态");
		},
		onError: (error) => {
			banner.showErrorBanner(error);
		},
	});

	const runQuery = useQuery({
		queryKey: ["awdp-run", runId],
		queryFn: () => awdpRunApi.getRun(runId),
	});
	const run = runQuery.data?.data;
	const phase = run?.phase;
	// 与 Challenge 练习一致：实例未运行（Pending 未 Launch / End 后）→ 只显示 Launch 按钮；
	// 实例运行中才显现时间面板与内容。
	const running = (run?.instances ?? []).some(
		(inst) => inst.runtime_state === "running",
	);
	const idle =
		(phase === "pending" || phase === "break" || phase === "fix") && !running;
	const needTimeline = phase === "fix" || phase === "ended";

	const roundsQuery = useQuery({
		queryKey: ["awdp-run-rounds", runId],
		queryFn: () => awdpRunApi.rounds(runId),
		enabled: needTimeline,
	});
	const evalsQuery = useQuery({
		queryKey: ["awdp-run-evals", runId],
		queryFn: () => awdpRunApi.evaluations(runId),
		enabled: needTimeline,
	});

	// GameBox 名称/分类由 run DTO 直接提供（后端实现补充字段）。
	const viewModel = useMemo<AwdpWorkbenchViewModel | null>(() => {
		if (!run) {
			return null;
		}
		return {
			title: run.gamebox_name,
			description: run.gamebox_description,
			phase: run.phase,
			startedAt: run.started_at,
			fixStartedAt: run.fix_started_at,
			breakDurationSecs: run.break_duration_secs,
			fixDurationSecs: run.fix_duration_secs,
			phaseEndsAt:
				run.phase === "break"
					? run.break_ends_at
					: run.phase === "fix"
						? run.next_action_at
						: null,
			breakEndsAt: run.break_ends_at,
			fixEndsAt: run.fix_ends_at,
			currentRound: run.current_round,
			totalRounds: run.total_rounds,
			nextCheckAt: run.phase === "fix" ? run.next_action_at : null,
			score: run.my_score,
			breakScore: run.break_score,
			fixRoundScore: run.fix_round_score,
			fixRoundPenalty: run.fix_round_penalty ?? 0,
			gameboxes: run.instances.map((inst) => ({
				id: inst.gamebox_id,
				gamebox_id: inst.gamebox_id,
				name: run.gamebox_name,
				category: run.gamebox_category,
				broken: inst.broken,
				enabled: true,
				source_code_dir: run.source_code_dir,
				instance: {
					instance_id: inst.instance_id,
					runtime_state: inst.runtime_state,
					runtime_generation: inst.runtime_generation,
					reset_count: inst.reset_count ?? 0,
					endpoints: inst.endpoints,
				},
			})),
			history: needTimeline
				? buildAwdpHistory(
						roundsQuery.data?.data ?? [],
						evalsQuery.data?.data ?? [],
						run.fix_round_score,
						run.fix_round_penalty,
					)
				: [],
			isPractice: true,
			canControlPhase: true,
			judgeEndpoint: run.judge_endpoint
				? {
						baseUrl: run.judge_endpoint.base_url,
						flagUrl: run.judge_endpoint.flag_url,
					}
				: null,
		};
	}, [run, roundsQuery.data, evalsQuery.data, needTimeline]);

	const invalidate = () => {
		queryClient.invalidateQueries({ queryKey: ["awdp-run", runId] });
		queryClient.invalidateQueries({ queryKey: ["awdp-run-rounds", runId] });
		queryClient.invalidateQueries({ queryKey: ["awdp-run-evals", runId] });
	};

	const callbacks = useMemo(
		() => ({
			onSubmitBreak: async (gameboxId: string, flag: string) => {
				const res = await awdpRunApi.submitBreak(runId, gameboxId, flag);
				invalidate();
				return res.data;
			},
			onUploadPatch: async (gameboxId: string, file: File) => {
				const res = await awdpRunApi.uploadPatch(runId, gameboxId, file);
				invalidate();
				return res.data;
			},
			onTestCheck: async (gameboxId: string) => {
				const res = await awdpRunApi.testCheck(runId, gameboxId);
				invalidate();
				// 同步 Test Check：POST 内直接执行完成（healthcheck + judge + exploit，
				// 不计分），无需再轮询 evaluations；旧响应保持兼容。
				if (res.data?.status === "completed") {
					return res.data;
				}
				// 异步 manual（旧后端）：轮询 evaluations 直到该评估终态（默认 ≤60s）。
				const evalId = res.data?.evaluation_id;
				if (!evalId) {
					return res.data;
				}
				const deadline = Date.now() + 60_000;
				for (;;) {
					await new Promise((r) => setTimeout(r, 2500));
					const res2 = await awdpRunApi.evaluations(runId);
					const evals = res2?.data ?? [];
					const ev = evals.find((e) => e.id === evalId);
					if (ev && ev.status !== "pending" && ev.status !== "running") {
						return {
							evaluation_id: evalId,
							status: ev.status,
							healthcheck_ok: ev.status !== "service_down",
							healthcheck_detail: ev.healthcheck_result
								? [ev.healthcheck_result]
								: [],
							judge_ok: ev.status !== "functional_broken",
							judge_detail: ev.judge_result ?? null,
							exploit_ok:
								ev.exploit_result == null ? null : ev.status === "vulnerable",
							exploit_detail: ev.exploit_result ?? null,
						};
					}
					if (Date.now() > deadline) {
						return res.data;
					}
				}
			},
			onAllCheck: async (gameboxId: string) => {
				const res = await awdpRunApi.allCheck(runId, gameboxId);
				invalidate();
				return res.data;
			},
			onSetPhase: async (target: "break" | "fix") => {
				const res = await awdpRunApi.setPhase(runId, target);
				if (res.data) {
					queryClient.setQueryData(["awdp-run", runId], res);
				}
				invalidate();
			},
			onEnd: async () => {
				await endMutation.mutateAsync();
			},
			onDownloadSource: async (gameboxId: string) => {
				const res = await awdpRunApi.sourceUrl(runId, gameboxId);
				return res.data;
			},
			onTrainAgain: async () => {
				const res = await awdpRunApi.restartTraining(runId);
				invalidate();
				const newRunId = res.data?.run_id;
				if (newRunId) {
					navigate({
						to: "/service/awdp/runs/$runId",
						params: { runId: newRunId },
					});
				}
			},
			onResetInstance: async (gameboxId: string) => {
				await awdpRunApi.resetInstance(runId, gameboxId);
				invalidate();
			},
		}),
		[runId, navigate, queryClient],
	);

	if (runQuery.isLoading) {
		return (
			<div className="p-4">
				<Spinner size="large" />
			</div>
		);
	}
	if (runQuery.isError) {
		return (
			<div className="p-4">
				<InlineMessage variant="critical">
					{(runQuery.error as Error)?.message ?? "Failed to load run."}
				</InlineMessage>
			</div>
		);
	}
	if (!run || !viewModel) {
		return (
			<div className="p-4">
				<InlineMessage variant="warning">Run not found.</InlineMessage>
			</div>
		);
	}

	// 与 Challenge 练习同款：未点「开始」（或 End 后）只显示标题 + 描述 + 「开始」按钮，
	// 面板与内容在点击开始（实例运行）后才显现。
	if (idle) {
		return (
			<div className="h-full w-full flex flex-col gap-2 justify-between min-h-0">
				<div id="awdp-meta" className="flex-6">
					<p className="font-bold text-2xl">{run.gamebox_name}</p>
					<div className="border-top mt-2 pt-2">
						{run.gamebox_description || "AWDP 训练场"}
					</div>
				</div>
				<banner.BannerComponent />
				<div
					id="awdp-content"
					className="mb-4 flex justify-center flex-1 border-bottom"
				>
					<Button
						variant="primary"
						disabled={startMutation.isPending}
						onClick={() => startMutation.mutate()}
					>
						{startMutation.isPending ? "Launching…" : "Launch"}
					</Button>
				</div>
			</div>
		);
	}

	return (
		<div className="h-full w-full flex flex-col min-h-0">
			<AwdpWorkbench viewModel={viewModel} {...callbacks} />
		</div>
	);
}
