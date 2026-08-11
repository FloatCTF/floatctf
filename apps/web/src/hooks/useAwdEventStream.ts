/**
 * AWD 实时事件流 Hook。
 *
 * 后端暴露 `/api/events/{id}/awd/stream` 时优先 EventSource/SSE。
 * 在接入 WS hub 前回退为 REST 快照轮询。
 */
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

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
	/** 为 true 时尝试 EventSource。默认 true。 */
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
	const [connected, setConnected] = useState(false);
	const [lastEvent, setLastEvent] = useState<AwdStreamEvent | null>(null);
	const lastSeq = useRef<number>(0);
	const seen = useRef<Set<number>>(new Set());

	const invalidateAwd = () => {
		qc.invalidateQueries({ queryKey: ["awd-scores", eventId] });
		qc.invalidateQueries({ queryKey: ["awd-gameboxes", eventId] });
		qc.invalidateQueries({ queryKey: ["admin-awd-scores", eventId] });
		qc.invalidateQueries({ queryKey: ["eventInfo", eventId] });
		qc.invalidateQueries({ queryKey: ["event", eventId] });
	};

	const handleEvent = (ev: AwdStreamEvent) => {
		if (typeof ev.sequence === "number") {
			if (seen.current.has(ev.sequence)) return;
			// 限制序号去重的内存占用
			if (seen.current.size > 2000) seen.current.clear();
			seen.current.add(ev.sequence);
			if (ev.sequence < lastSeq.current) {
				// 可能发生重连回退——刷新快照
				invalidateAwd();
			}
			lastSeq.current = Math.max(lastSeq.current, ev.sequence);
		}
		setLastEvent(ev);
		// 任意比分/轮次/网络变更 → REST 快照刷新
		if (
			ev.type.startsWith("score.") ||
			ev.type.startsWith("attack.") ||
			ev.type.startsWith("judge.") ||
			ev.type.startsWith("round.") ||
			ev.type.includes("pause") ||
			ev.type.includes("resume") ||
			ev.type.includes("ban") ||
			ev.type.includes("network") ||
			ev.type.includes("precheck")
		) {
			invalidateAwd();
		}
	};

	useEffect(() => {
		if (!enabled || !eventId) return;

		let es: EventSource | null = null;
		let pollTimer: ReturnType<typeof setInterval> | null = null;
		let closed = false;

		const startPoll = () => {
			if (pollTimer || closed) return;
			setConnected(false);
			pollTimer = setInterval(invalidateAwd, pollMs);
			// 回退路径立即拉快照
			invalidateAwd();
		};

		if (preferStream && typeof EventSource !== "undefined") {
			try {
				// 后端可能尚未暴露；onerror 回退为轮询。
				es = new EventSource(`/api/events/${eventId}/awd/stream`, {
					withCredentials: true,
				});
				es.onopen = () => {
					if (!closed) setConnected(true);
				};
				es.onmessage = (msg) => {
					try {
						const data = JSON.parse(msg.data) as AwdStreamEvent;
						handleEvent(data);
					} catch {
						// 忽略格式错误
					}
				};
				es.onerror = () => {
					es?.close();
					es = null;
					startPoll();
				};
			} catch {
				startPoll();
			}
		} else {
			startPoll();
		}

		return () => {
			closed = true;
			es?.close();
			if (pollTimer) clearInterval(pollTimer);
		};
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [eventId, enabled, pollMs, preferStream]);

	return { connected, lastEvent, invalidateAwd };
}
