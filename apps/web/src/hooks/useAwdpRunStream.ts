import { useQueryClient } from "@tanstack/react-query";
/**
 * AWDP Practice Run 实时事件流 hook。
 *
 * 与 useAwdpEventStream 同模式：优先 EventSource
 * （`/api/service/awdp/runs/{runId}/stream`），断线回退轮询 invalidate。
 * 命中 `awdp.(score|phase|patch|manual|round|evaluation|instance|run)` 时
 * 批量失效 run 页相关 queryKey（含目录页 active_training）。
 */
import { useCallback, useEffect, useRef, useState } from "react";

import type { AwdpStreamEvent } from "@/hooks/useAwdpEventStream";

export type UseAwdpRunStreamOptions = {
	runId: string;
	pollMs?: number;
	preferStream?: boolean;
	enabled?: boolean;
};

const SNAPSHOT_RE =
	/awdp\.(score|phase|patch|manual|round|evaluation|instance|run)/;

export function useAwdpRunStream({
	runId,
	pollMs = 15000,
	preferStream = true,
	enabled = true,
}: UseAwdpRunStreamOptions) {
	const [connected, setConnected] = useState(false);
	const lastEventRef = useRef<AwdpStreamEvent | null>(null);
	const queryClient = useQueryClient();

	const invalidate = useCallback(() => {
		queryClient.invalidateQueries({ queryKey: ["awdp-run", runId] });
		queryClient.invalidateQueries({ queryKey: ["awdp-run-rounds", runId] });
		queryClient.invalidateQueries({ queryKey: ["awdp-run-evals", runId] });
		queryClient.invalidateQueries({ queryKey: ["awdp-run-scores", runId] });
		// 目录页 active_training 的 phase/run 会随 run 推进变化。
		queryClient.invalidateQueries({ queryKey: ["gamebox-catalog"] });
	}, [queryClient, runId]);

	const onEvent = useCallback(
		(ev: AwdpStreamEvent) => {
			lastEventRef.current = ev;
			if (SNAPSHOT_RE.test(ev.type)) {
				invalidate();
			}
		},
		[invalidate],
	);

	useEffect(() => {
		if (!enabled || !runId) {
			return;
		}
		let es: EventSource | null = null;
		let pollTimer: ReturnType<typeof setInterval> | null = null;
		let stopped = false;

		const startPolling = () => {
			if (stopped || pollTimer) {
				return;
			}
			pollTimer = setInterval(() => {
				if (!stopped) {
					invalidate();
				}
			}, pollMs);
		};

		if (preferStream && typeof EventSource !== "undefined") {
			es = new EventSource(`/api/service/awdp/runs/${runId}/stream`, {
				withCredentials: true,
			});
			es.onopen = () => setConnected(true);
			es.onerror = () => {
				setConnected(false);
				es?.close();
				startPolling();
			};
			es.onmessage = (msg) => {
				try {
					const data = JSON.parse(msg.data) as AwdpStreamEvent;
					if (data && typeof data === "object" && "type" in data) {
						onEvent(data);
					}
				} catch {
					// ignore malformed frames
				}
			};
		} else {
			startPolling();
		}

		return () => {
			stopped = true;
			es?.close();
			if (pollTimer) {
				clearInterval(pollTimer);
			}
		};
	}, [runId, enabled, preferStream, pollMs, invalidate, onEvent]);

	return {
		connected,
		lastEvent: lastEventRef.current,
		invalidateRun: invalidate,
	};
}
