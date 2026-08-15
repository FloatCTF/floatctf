import { useQueryClient } from "@tanstack/react-query";
/**
 * AWDP 实时事件流 hook。
 * 与 useAwdEventStream 同模式：优先 EventSource，失败回退轮询（invalidate）。
 */
import { useCallback, useEffect, useRef, useState } from "react";

export type AwdpStreamEvent = {
	type: string;
	sequence?: number;
	payload?: unknown;
	occurred_at?: string;
};

export type UseAwdpEventStreamOptions = {
	eventId: string;
	pollMs?: number;
	preferStream?: boolean;
	enabled?: boolean;
};

const SNAPSHOT_RE =
	/awdp\.(score|phase|config|event|patch|manual|round|evaluation|instance)/;

export function useAwdpEventStream({
	eventId,
	pollMs = 15000,
	preferStream = true,
	enabled = true,
}: UseAwdpEventStreamOptions) {
	const [connected, setConnected] = useState(false);
	const lastEventRef = useRef<AwdpStreamEvent | null>(null);
	const queryClient = useQueryClient();
	// 事件节流：比赛进行中 score/phase 事件可能密集（多人抢 flag），
	// 把短窗口内多次 invalidate 合并为一次，避免 refetch 风暴卡住页面。
	const invalidateTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

	const invalidate = useCallback(() => {
		if (invalidateTimerRef.current) {
			return;
		}
		invalidateTimerRef.current = setTimeout(() => {
			invalidateTimerRef.current = null;
			queryClient.invalidateQueries({ queryKey: ["awdp-overview", eventId] });
			queryClient.invalidateQueries({ queryKey: ["awdp-config", eventId] });
			queryClient.invalidateQueries({ queryKey: ["awdp-rounds", eventId] });
			queryClient.invalidateQueries({ queryKey: ["awdp-evals", eventId] });
			queryClient.invalidateQueries({ queryKey: ["awdp-scoreboard", eventId] });
			queryClient.invalidateQueries({ queryKey: ["eventInfo", eventId] });
			queryClient.invalidateQueries({ queryKey: ["event", eventId] });
		}, 1000);
	}, [queryClient, eventId]);

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
		if (!enabled || !eventId) {
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
			es = new EventSource(`/api/events/${eventId}/awdp/stream`, {
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
			if (invalidateTimerRef.current) {
				clearTimeout(invalidateTimerRef.current);
				invalidateTimerRef.current = null;
			}
		};
	}, [eventId, enabled, preferStream, pollMs, invalidate, onEvent]);

	return {
		connected,
		lastEvent: lastEventRef.current,
		invalidateAwdp: invalidate,
	};
}
