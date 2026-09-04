/**
 * Admin AWD 实时事件流 Hook。
 *
 * 使用管理端认证令牌（adminToken）连接 `/api/admin/events/{id}/awd/stream`。
 * 认证：SuperAdminJwtGuard（super_admin 表，独立于 users 认证域）。
 *
 * 与 `useAwdEventStream` 共享 `connectSse` 传输层，仅 URL 和 token 来源不同。
 */
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { useAuthStore } from "@/stores/AuthStore";
import {
	type SseConnection,
	type SseConnectionState,
	type SseEvent,
	connectSse,
} from "@/lib/sse";

export type AwdStreamEvent = {
	type: string;
	sequence?: number;
	payload?: unknown;
	occurred_at?: string;
};

export type UseAdminAwdEventStreamOptions = {
	eventId: string;
	pollMs?: number;
	preferStream?: boolean;
	enabled?: boolean;
};

export function useAdminAwdEventStream(options: UseAdminAwdEventStreamOptions) {
	const {
		eventId,
		pollMs = 15_000,
		preferStream = true,
		enabled = true,
	} = options;

	// 管理端令牌 — 独立于 users 认证域
	const adminToken = useAuthStore((s) => s.adminToken);

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

				if (typeof data.sequence === "number") {
					if (seen.current.has(data.sequence)) return;
					if (seen.current.size > 2000) seen.current.clear();
					seen.current.add(data.sequence);
					if (data.sequence < lastSeq.current) {
						invalidateAwd();
					}
					lastSeq.current = Math.max(lastSeq.current, data.sequence);
				}

				setLastEvent(data);

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

		// ── REST 轮询回退（SSE 不可用时保证权威状态持续更新）──
		// Phase 9.2 A1：非 connected 状态一律启动轮询；connected 恢复后停止。
		const stopPoll = () => {
			if (pollTimer) {
				clearInterval(pollTimer);
				pollTimer = null;
			}
		};

		const ensurePoll = () => {
			if (pollTimer || disposed) return;
			pollTimer = setInterval(invalidateAwd, pollMs);
			invalidateAwd();
		};

		// 无管理端令牌 → 回退轮询
		if (!adminToken) {
			setConnectionState("idle");
			ensurePoll();
			return () => {
				disposed = true;
				stopPoll();
			};
		}

		if (preferStream) {
			const controller = new AbortController();

			const connection = connectSse({
				url: `/api/admin/events/${eventId}/awd/stream`,
				headers: {},
				signal: controller.signal,
				getToken: () => adminToken,
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
						// 断线回退：非 connected → 轮询；恢复 → 停轮询。
						if (status.state === "connected") {
							stopPoll();
						} else {
							ensurePoll();
						}
						if (status.state === "auth_error") {
							connRef.current?.close();
							connRef.current = null;
							ensurePoll();
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
				stopPoll();
			};
		}

		setConnectionState("idle");
		ensurePoll();

		return () => {
			disposed = true;
			stopPoll();
		};
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [eventId, enabled, pollMs, preferStream, adminToken, handleSseEvent, invalidateAwd]);

	return {
		connected: connectionState === "connected",
		connectionState,
		lastEvent,
		lastError,
		invalidateAwd,
	};
}