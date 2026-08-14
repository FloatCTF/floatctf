import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";

import { type AwdpOverview, awdpPlayerApi } from "@/api/awdp";
import {
	AwdpWorkbench,
	type AwdpWorkbenchViewModel,
	buildAwdpHistory,
} from "@/components/awdp/AwdpWorkbench";

/**
 * 赛事（Competition）→ AwdpWorkbench 适配器（§65）。
 *
 * 把 event-scoped `AwdpOverview` 映射为统一 view-model，并接线 event 版
 * API 回调（mutations 后失效 overview/rounds/evals）。
 * 赛事默认落地页（index.tsx，Overview）与 gameboxes.tsx（GameBoxes tab）
 * 均渲染本组件，不复制两套页面实现；顶部进度条为事件级共享。
 */
export function AwdpEventWorkbench({ eventId }: { eventId: string }) {
	const queryClient = useQueryClient();

	const overviewQuery = useQuery({
		queryKey: ["awdp-overview", eventId],
		queryFn: () => awdpPlayerApi.overview(eventId),
	});
	const overview = overviewQuery.data?.data;
	const phase = overview?.phase;
	const needTimeline = phase === "fix" || phase === "ended";

	const roundsQuery = useQuery({
		queryKey: ["awdp-rounds", eventId],
		queryFn: () => awdpPlayerApi.rounds(eventId),
		enabled: needTimeline,
	});
	const evalsQuery = useQuery({
		queryKey: ["awdp-evals", eventId],
		queryFn: () => awdpPlayerApi.evaluations(eventId),
		enabled: needTimeline,
	});

	const viewModel = useMemo<AwdpWorkbenchViewModel | null>(() => {
		if (!overview) {
			return null;
		}
		return toViewModel(
			overview,
			needTimeline ? (roundsQuery.data?.data ?? []) : [],
			needTimeline ? (evalsQuery.data?.data ?? []) : [],
		);
	}, [overview, needTimeline, roundsQuery.data, evalsQuery.data]);

	const invalidate = () => {
		queryClient.invalidateQueries({ queryKey: ["awdp-overview", eventId] });
		queryClient.invalidateQueries({ queryKey: ["awdp-rounds", eventId] });
		queryClient.invalidateQueries({ queryKey: ["awdp-evals", eventId] });
	};

	const callbacks = useMemo(
		() => ({
			onSubmitBreak: async (egId: string, flag: string) => {
				const res = await awdpPlayerApi.submitBreak(eventId, egId, flag);
				invalidate();
				return res.data;
			},
			onUploadPatch: async (egId: string, file: File) => {
				const res = await awdpPlayerApi.uploadPatch(eventId, egId, file);
				invalidate();
				return res.data;
			},
			onTestCheck: async (egId: string) => {
				const res = await awdpPlayerApi.testCheck(eventId, egId);
				invalidate();
				return res.data;
			},
			onDownloadSource: async (egId: string) => {
				const res = await awdpPlayerApi.sourceUrl(eventId, egId);
				return res.data;
			},
			onStartInstance: async (egId: string) => {
				await awdpPlayerApi.startInstance(eventId, egId);
				invalidate();
			},
			onStopInstance: async (egId: string) => {
				await awdpPlayerApi.stopInstance(eventId, egId);
				invalidate();
			},
			onResetInstance: async (egId: string) => {
				await awdpPlayerApi.resetInstance(eventId, egId);
				invalidate();
			},
		}),
		[viewModel, eventId, queryClient],
	);

	if (!overview || !viewModel) {
		return null;
	}

	return <AwdpWorkbench viewModel={viewModel} {...callbacks} />;
}

function toViewModel(
	overview: AwdpOverview,
	rounds: {
		sequence: number;
		starts_at: string;
		cutoff_at: string;
		status: string;
	}[],
	evals: {
		round_sequence: number | null;
		kind: string;
		status: string;
		finished_at: string | null;
	}[],
): AwdpWorkbenchViewModel {
	const phase = overview.phase;
	return {
		title: "",
		phase,
		startedAt: overview.started_at,
		fixStartedAt: overview.fix_started_at,
		breakDurationSecs: overview.break_duration_secs,
		fixDurationSecs: overview.fix_duration_secs,
		phaseEndsAt:
			phase === "break"
				? overview.break_ends_at
				: phase === "fix"
					? overview.next_action_at
					: null,
		breakEndsAt: overview.break_ends_at,
		fixEndsAt: overview.fix_ends_at,
		currentRound: overview.current_round,
		totalRounds: overview.total_rounds,
		nextCheckAt: phase === "fix" ? overview.next_action_at : null,
		score: overview.my_score,
		breakScore: overview.break_score,
		fixRoundScore: overview.fix_round_score,
		gameboxes: overview.gameboxes.map((gb) => ({
			id: gb.id,
			gamebox_id: gb.gamebox_id,
			name: gb.name,
			category: gb.category,
			exposed: gb.exposed,
			broken: gb.broken,
			enabled: gb.enabled,
			source_code_dir: gb.source_code_dir ?? null,
			instance: gb.instance
				? {
						instance_id: gb.instance.instance_id,
						runtime_state: gb.instance.runtime_state,
						runtime_generation: gb.instance.runtime_generation,
						endpoints: gb.instance.endpoints,
					}
				: null,
		})),
		history: buildAwdpHistory(rounds, evals, overview.fix_round_score),
		isPractice: false,
	};
}
