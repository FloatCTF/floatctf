import { Spinner } from "@primer/react";
import { InlineMessage } from "@primer/react/experimental";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useTitle } from "ahooks";
import { useMemo } from "react";

import { awdpRunApi } from "@/api/awdpRuns";
import {
	AwdpWorkbench,
	type AwdpWorkbenchViewModel,
	buildAwdpHistory,
} from "@/components/awdp/AwdpWorkbench";
import { useAwdpRunStream } from "@/hooks/useAwdpRunStream";
import { ServiceRouteGuard } from "../route";

export const Route = createFileRoute("/service/awdp/runs/$runId")({
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
	const stream = useAwdpRunStream({ runId });

	const runQuery = useQuery({
		queryKey: ["awdp-run", runId],
		queryFn: () => awdpRunApi.getRun(runId),
	});
	const run = runQuery.data?.data;
	const phase = run?.phase;
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
	const scoresQuery = useQuery({
		queryKey: ["awdp-run-scores", runId],
		queryFn: () => awdpRunApi.scores(runId),
		enabled: phase === "ended",
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
					endpoints: inst.endpoints,
				},
			})),
			history: needTimeline
				? buildAwdpHistory(
						roundsQuery.data?.data ?? [],
						evalsQuery.data?.data ?? [],
						run.fix_round_score,
					)
				: [],
			scoreHistory: scoresQuery.data?.data?.history ?? [],
			isPractice: true,
		};
	}, [run, roundsQuery.data, evalsQuery.data, scoresQuery.data, needTimeline]);

	const invalidate = () => {
		queryClient.invalidateQueries({ queryKey: ["awdp-run", runId] });
		queryClient.invalidateQueries({ queryKey: ["awdp-run-rounds", runId] });
		queryClient.invalidateQueries({ queryKey: ["awdp-run-evals", runId] });
		queryClient.invalidateQueries({ queryKey: ["awdp-run-scores", runId] });
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
				return res.data;
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
			onStartInstance: async (gameboxId: string) => {
				await awdpRunApi.startInstance(runId, gameboxId);
				invalidate();
			},
			onStopInstance: async (gameboxId: string) => {
				await awdpRunApi.stopInstance(runId, gameboxId);
				invalidate();
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

	return (
		<div className="m-2 flex flex-col gap-2">
			<div className="flex items-center gap-2">
				<span className="text-xs opacity-60 ml-auto">
					{stream.connected ? "live" : "poll"}
				</span>
			</div>
			<AwdpWorkbench viewModel={viewModel} {...callbacks} />
		</div>
	);
}
