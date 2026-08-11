/**
 * AWDP 实时事件流 hook。
 * 与 useAwdEventStream 同模式：优先 EventSource，失败回退轮询（invalidate）。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

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

const SNAPSHOT_RE = /awdp\.(score|phase|patch|manual|round|evaluation|instance)/;

export function useAwdpEventStream({
	eventId,
	pollMs = 15000,
	preferStream = true,
	enabled = true,
}: UseAwdpEventStreamOptions) {
	const [connected, setConnected] = useState(false);
	const lastEventRef = useRef<AwdpStreamEvent | null>(null);
	const queryClient = useQueryClient();

	const invalidate = useCallback(() => {
		queryClient.invalidateQueries({ queryKey: ["awdp-overview", eventId] });
		queryClient.invalidateQueries({ queryKey: ["awdp-config", eventId] });
		queryClient.invalidateQueries({ queryKey: ["eventInfo", eventId] });
		queryClient.invalidateQueries({ queryKey: ["event", eventId] });
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
		};
	}, [eventId, enabled, preferStream, pollMs, invalidate, onEvent]);

	return { connected, lastEvent: lastEventRef.current, invalidateAwdp: invalidate };
}
