import {
	type SseConnection,
	type SseConnectionState,
	type SseEvent,
	connectSse,
} from "@/lib/sse";
import { useAuthStore } from "@/stores/AuthStore";
/**
 * AWD 实时事件流 Hook。
 *
 * 使用 fetch-based SSE（`connectSse`）连接 `/api/events/{id}/awd/stream`，
 * 通过 Authorization: Bearer 头传递认证令牌。
 *
 * 原生 EventSource 无法发送自定义请求头，因此本 hook 不再使用 EventSource。
 *
 * 连接断开时自动重连（指数退避 + 抖动），认证失败 (401/403) 停止重连。
 * 重连后若后端不支持 Last-Event-ID 重放，触发 REST 快照刷新。
 */
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";

export type AwdStreamEvent = {
	type: string;
	sequence?: number;
	payload?: unknown;
	occurred_at?: string;
};

export type UseAwdEventStreamOptions = {
	eventId: string;
	/** 流不可用时的 REST 快照间隔（毫秒）。默认 15000。 */
	pollMs?: number;
	/** 为 true 时尝试 SSE。默认 true。 */
	preferStream?: boolean;
	enabled?: boolean;
};

export function useAwdEventStream(options: UseAwdEventStreamOptions) {
	const {
		eventId,
		pollMs = 15_000,
		preferStream = true,
		enabled = true,
	} = options;
	const qc = useQueryClient();
	const [connectionState, setConnectionState] =
		useState<SseConnectionState>("idle");
	const [lastEvent, setLastEvent] = useState<AwdStreamEvent | null>(null);
	const [lastError, setLastError] = useState<Error | null>(null);
	const lastSeq = useRef<number>(0);
	const seen = useRef<Set<number>>(new Set());
	const connRef = useRef<SseConnection | null>(null);

	const invalidateAwd = useCallback(() => {
		qc.invalidateQueries({ queryKey: ["awd-scores", eventId] });
		qc.invalidateQueries({ queryKey: ["awd-gameboxes", eventId] });
		qc.invalidateQueries({ queryKey: ["admin-awd-scores", eventId] });
		qc.invalidateQueries({ queryKey: ["eventInfo", eventId] });
		qc.invalidateQueries({ queryKey: ["event", eventId] });
	}, [qc, eventId]);

	const handleSseEvent = useCallback(
		(ev: SseEvent) => {
			try {
				const data = JSON.parse(ev.data) as AwdStreamEvent;
				if (!data || typeof data !== "object" || !("type" in data)) return;

				// 序列号去重
				if (typeof data.sequence === "number") {
					if (seen.current.has(data.sequence)) return;
					if (seen.current.size > 2000) seen.current.clear();
					seen.current.add(data.sequence);
					if (data.sequence < lastSeq.current) {
						// 重连回退 → 刷新快照
						invalidateAwd();
					}
					lastSeq.current = Math.max(lastSeq.current, data.sequence);
				}

				setLastEvent(data);

				// 比分/轮次/网络变更 → REST 快照刷新
				if (
					data.type.startsWith("score.") ||
					data.type.startsWith("attack.") ||
					data.type.startsWith("judge.") ||
					data.type.startsWith("round.") ||
					data.type.includes("pause") ||
					data.type.includes("resume") ||
					data.type.includes("ban") ||
					data.type.includes("network") ||
					data.type.includes("precheck")
				) {
					invalidateAwd();
				}
			} catch {
				// 忽略格式错误
			}
		},
		[invalidateAwd],
	);

	useEffect(() => {
		if (!enabled || !eventId) {
			connRef.current?.close();
			connRef.current = null;
			return;
		}

		let pollTimer: ReturnType<typeof setInterval> | null = null;
		let disposed = false;

		const startPoll = () => {
			if (pollTimer || disposed) return;
			setConnectionState("idle");
			pollTimer = setInterval(invalidateAwd, pollMs);
			invalidateAwd();
		};

		if (preferStream) {
			const controller = new AbortController();

			const connection = connectSse({
				url: `/api/events/${eventId}/awd/stream`,
				headers: {},
				signal: controller.signal,
				getToken: () => useAuthStore.getState().token,
				onOpen: () => {
					if (!disposed) setConnectionState("connected");
				},
				onEvent: handleSseEvent,
				onError: (err) => {
					if (!disposed) setLastError(err);
				},
				onStateChange: (status) => {
					if (!disposed) {
						setConnectionState(status.state);
						if (status.lastError) setLastError(status.lastError);
						// 认证失败 → 回退轮询
						if (status.state === "auth_error") {
							connRef.current?.close();
							connRef.current = null;
							startPoll();
						}
					}
				},
			});

			connRef.current = connection;

			return () => {
				disposed = true;
				controller.abort();
				connection.close();
				connRef.current = null;
				if (pollTimer) {
					clearInterval(pollTimer);
					pollTimer = null;
				}
			};
		}

		startPoll();

		return () => {
			disposed = true;
			if (pollTimer) {
				clearInterval(pollTimer);
				pollTimer = null;
			}
		};
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [eventId, enabled, pollMs, preferStream, handleSseEvent, invalidateAwd]);

	return {
		connected: connectionState === "connected",
		connectionState,
		lastEvent,
		lastError,
		invalidateAwd,
	};
}
