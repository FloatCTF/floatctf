import {
	type SseConnection,
	type SseConnectionState,
	type SseEvent,
	connectSse,
} from "@/lib/sse";
import { useAuthStore } from "@/stores/AuthStore";
import { useQueryClient } from "@tanstack/react-query";
/**
 * AWDP Practice Run 实时事件流 hook。
 *
 * 使用 fetch-based SSE 通过 Authorization: Bearer 头传递认证令牌。
 * 连接 `/api/service/awdp/runs/{runId}/stream`，断线回退轮询 invalidate。
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
	const token = useAuthStore((s) => s.token);

	const [connectionState, setConnectionState] =
		useState<SseConnectionState>("idle");
	const [lastError, setLastError] = useState<Error | null>(null);
	const lastEventRef = useRef<AwdpStreamEvent | null>(null);
	const queryClient = useQueryClient();
	const connRef = useRef<SseConnection | null>(null);

	const invalidate = useCallback(() => {
		queryClient.invalidateQueries({ queryKey: ["awdp-run", runId] });
		queryClient.invalidateQueries({ queryKey: ["awdp-run-rounds", runId] });
		queryClient.invalidateQueries({ queryKey: ["awdp-run-evals", runId] });
		queryClient.invalidateQueries({ queryKey: ["awdp-run-scores", runId] });
		queryClient.invalidateQueries({ queryKey: ["gamebox-catalog"] });
	}, [queryClient, runId]);

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
		if (!enabled || !runId) {
			connRef.current?.close();
			connRef.current = null;
			return;
		}
		let pollTimer: ReturnType<typeof setInterval> | null = null;
		let stopped = false;

		// ── REST 轮询回退（SSE 不可用时保证权威状态持续更新）──
		// Phase 9.2 A1：非 connected 状态一律启动轮询；connected 恢复后停止。
		const stopPolling = () => {
			if (pollTimer) {
				clearInterval(pollTimer);
				pollTimer = null;
			}
		};

		const ensurePolling = () => {
			if (stopped || pollTimer) {
				return;
			}
			pollTimer = setInterval(() => {
				if (!stopped) {
					invalidate();
				}
			}, pollMs);
		};

		if (!token) {
			ensurePolling();
			return () => {
				stopped = true;
				stopPolling();
			};
		}

		if (preferStream) {
			const controller = new AbortController();

			const connection = connectSse({
				url: `/api/service/awdp/runs/${runId}/stream`,
				headers: {},
				signal: controller.signal,
				getToken: () => token,
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
						// 断线回退：非 connected → 轮询；恢复 → 停轮询。
						if (status.state === "connected") {
							stopPolling();
						} else {
							ensurePolling();
						}
						if (status.state === "auth_error") {
							connRef.current?.close();
							connRef.current = null;
							ensurePolling();
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
				stopPolling();
			};
		}

		ensurePolling();

		return () => {
			stopped = true;
			stopPolling();
		};
	}, [runId, enabled, preferStream, pollMs, token, invalidate, onEvent]);

	return {
		connected: connectionState === "connected",
		connectionState,
		lastEvent: lastEventRef.current,
		lastError,
		invalidateRun: invalidate,
	};
}
