import {
	type SseConnection,
	type SseConnectionState,
	type SseEvent,
	connectSse,
} from "@/lib/sse";
import { useAuthStore } from "@/stores/AuthStore";
import { useQueryClient } from "@tanstack/react-query";
/**
 * AWDP 实时事件流 hook。
 * 使用 fetch-based SSE 通过 Authorization: Bearer 头传递认证令牌。
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
	const [connectionState, setConnectionState] =
		useState<SseConnectionState>("idle");
	const [lastError, setLastError] = useState<Error | null>(null);
	const lastEventRef = useRef<AwdpStreamEvent | null>(null);
	const queryClient = useQueryClient();
	const connRef = useRef<SseConnection | null>(null);
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
			queryClient.invalidateQueries({ queryKey: ["awdp-trend", eventId] });
			queryClient.invalidateQueries({ queryKey: ["eventInfo", eventId] });
			queryClient.invalidateQueries({ queryKey: ["event", eventId] });
		}, 1000);
	}, [queryClient, eventId]);

	const onEvent = useCallback(
		(ev: SseEvent) => {
			try {
				const data = JSON.parse(ev.data) as AwdpStreamEvent;
				if (data && typeof data === "object" && "type" in data) {
					lastEventRef.current = data;
					if (SNAPSHOT_RE.test(data.type)) {
						invalidate();
					}
				}
			} catch {
				// ignore malformed frames
			}
		},
		[invalidate],
	);

	useEffect(() => {
		if (!enabled || !eventId) {
			connRef.current?.close();
			connRef.current = null;
			return;
		}
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

		if (preferStream) {
			const controller = new AbortController();

			const connection = connectSse({
				url: `/api/events/${eventId}/awdp/stream`,
				headers: {},
				signal: controller.signal,
				getToken: () => useAuthStore.getState().token,
				onOpen: () => {
					if (!stopped) setConnectionState("connected");
				},
				onEvent,
				onError: (err) => {
					if (!stopped) setLastError(err);
				},
				onStateChange: (status) => {
					if (!stopped) {
						setConnectionState(status.state);
						if (status.lastError) setLastError(status.lastError);
						if (status.state === "auth_error") {
							connRef.current?.close();
							connRef.current = null;
							startPolling();
						}
					}
				},
			});

			connRef.current = connection;

			return () => {
				stopped = true;
				controller.abort();
				connection.close();
				connRef.current = null;
				if (pollTimer) {
					clearInterval(pollTimer);
				}
				if (invalidateTimerRef.current) {
					clearTimeout(invalidateTimerRef.current);
					invalidateTimerRef.current = null;
				}
			};
		}

		startPolling();

		return () => {
			stopped = true;
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
		connected: connectionState === "connected",
		connectionState,
		lastEvent: lastEventRef.current,
		lastError,
		invalidateAwdp: invalidate,
	};
}
