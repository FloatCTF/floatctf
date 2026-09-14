import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";

import { type AwdpOverview, awdpPlayerApi } from "@/api/awdp";
import { AwdpWorkbench, type AwdpWorkbenchViewModel } from "@/components/awdp/AwdpWorkbench";

/**
 * 赛事（Competition）→ AwdpWorkbench 适配器（§65）。
 *
 * 把 event-scoped `AwdpOverview` 映射为统一 view-model，并接线 event 版
 * API 回调（mutations 后失效 overview/rounds/evals）。
 * 赛事 GameBoxes tab 渲染本组件；顶部进度条为事件级共享。
 * 注意：赛事不再渲染 Official History / Final Score（由 Rounds / Scoreboard
 * tab 承载），故不在此预取 rounds/evals——history 恒为空。
 */
export function AwdpEventWorkbench({ eventId }: { eventId: string }) {
	const queryClient = useQueryClient();

	const overviewQuery = useQuery({
		queryKey: ["awdp-overview", eventId],
		queryFn: () => awdpPlayerApi.overview(eventId),
	});
	const overview = overviewQuery.data?.data;

	const viewModel = useMemo<AwdpWorkbenchViewModel | null>(() => {
		if (!overview) {
			return null;
		}
		return toViewModel(overview);
	}, [overview]);

	const invalidate = () => {
		queryClient.invalidateQueries({ queryKey: ["awdp-overview", eventId] });
		// rounds/evals 由 Rounds tab 自己订阅；这里一并失效保持新鲜。
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

function toViewModel(overview: AwdpOverview): AwdpWorkbenchViewModel {
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
						reset_count: gb.instance.reset_count ?? 0,
						endpoints: gb.instance.endpoints,
					}
				: null,
		})),
		history: [],
		isPractice: false,
	};
}
