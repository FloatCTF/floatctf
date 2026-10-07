import type {
	FloatCTFClient,
	SseConnection,
	SseConnectionState,
	SseEvent,
} from "@floatctf/sdk";
/**
 * AWD 实时事件流 Hook。
 *
 * 使用 `client.sse.connect()`（fetch-based SSE）连接
 * `{client.baseUrl}/events/{id}/awd/stream`，通过 Authorization: Bearer 头传递认证令牌。
 * URL 由**客户端自己的 base URL** 决定（绝不硬编码 `/api`），因此跨源/自建 base URL 的前端
 * 与 REST 请求走同一个地址。
 *
 * 原生 EventSource 无法发送自定义请求头，因此本 hook 不再使用 EventSource。
 *
 * 连接断开时自动重连（指数退避 + 抖动），认证失败 (401/403) 停止重连。
 * 重连后若后端不支持 Last-Event-ID 重放，触发 REST 快照刷新。
 *
 * 令牌生命周期：
 * - 令牌变更 → 旧连接清理，新连接创建
 * - 令牌变为 null（登出）→ 中止 SSE，回退轮询
 * - 令牌从 null 变为有效 → 创建 SSE 连接
 * - 401/403 → auth_error，停止重连；令牌变更后重新尝试
 */
import { useQueryClient } from "@tanstack/react-query";

import type { UseTokenSource } from "./tokenSource.js";
import { useCallback, useEffect, useRef, useState } from "react";

import {
	AWD_PLAYER_QUERY_KEYS,
	invalidateAwdQueries,
} from "./awdInvalidation.js";

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

export function createUseAwdEventStream(client: FloatCTFClient, useUserToken: UseTokenSource) {
	return function useAwdEventStream(options: UseAwdEventStreamOptions) {
	const {
		eventId,
		pollMs = 15_000,
		preferStream = true,
		enabled = true,
	} = options;

	// 订阅令牌变更 — 令牌变化会触发 effect 清理 + 重建
	const token = useUserToken();

	const qc = useQueryClient();
	const [connectionState, setConnectionState] =
		useState<SseConnectionState>("idle");
	const [lastEvent, setLastEvent] = useState<AwdStreamEvent | null>(null);
	const [lastError, setLastError] = useState<Error | null>(null);
	const lastSeq = useRef<number>(0);
	const seen = useRef<Set<number>>(new Set());
	const connRef = useRef<SseConnection | null>(null);

	const invalidateAwd = useCallback(() => {
		// 与选手页面手动刷新共用同一份 key 常量（见 awdInvalidation.ts）：
		// 此前只有 5 个 key，比页面实际读取的窄，SSE 生效后这些面板反而不刷新了。
		invalidateAwdQueries(qc, eventId, AWD_PLAYER_QUERY_KEYS);
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

	// token 在依赖数组中 — 令牌变更时自动清理旧连接并创建新连接
	useEffect(() => {
		if (!enabled || !eventId) {
			connRef.current?.close();
			connRef.current = null;
			return;
		}

		let pollTimer: ReturnType<typeof setInterval> | null = null;
		let disposed = false;

		// ── REST 轮询回退（SSE 不可用时保证权威状态持续更新）──
		// Phase 9.2 A1：非 connected 状态（connecting/reconnecting/error/
		// auth_error）一律启动轮询；connected 恢复后立即停止轮询。
		// 断线期间页面不冻结（REST 快照持续更新）；SSE 恢复后恰好一条
		// SSE 通道生效、无重复事件处理。
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

		// 无令牌 → 回退轮询（不尝试未认证的 SSE）
		if (!token) {
			setConnectionState("idle");
			ensurePoll();
			return () => {
				disposed = true;
				stopPoll();
			};
		}

		if (preferStream) {
			const controller = new AbortController();

			const connection = client.sse.connect({
				url: `/events/${eventId}/awd/stream`,
				headers: {},
				signal: controller.signal,
				// 使用闭包捕获的 token（effect 重建时更新）
				getToken: () => token,
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
		// token 在依赖数组中 → 令牌变更触发清理 + 重建
			}, [client, eventId, enabled, pollMs, preferStream, token, handleSseEvent, invalidateAwd]);

	return {
		connected: connectionState === "connected",
		connectionState,
		lastEvent,
		lastError,
		invalidateAwd,
	};
	};
}
