/**
 * AWD realtime event stream hook.
 *
 * Prefers EventSource/SSE when backend exposes `/api/events/{id}/awd/stream`.
 * Falls back to REST snapshot polling until a WS hub is fronted.
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
	/** REST snapshot interval when stream is unavailable (ms). Default 15000. */
	pollMs?: number;
	/** When true, enable EventSource attempt. Default true. */
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
			// Cap memory for sequence de-dupe
			if (seen.current.size > 2000) seen.current.clear();
			seen.current.add(ev.sequence);
			if (ev.sequence < lastSeq.current) {
				// Possible reconnect rewind — refresh snapshot
				invalidateAwd();
			}
			lastSeq.current = Math.max(lastSeq.current, ev.sequence);
		}
		setLastEvent(ev);
		// Any score/round/network change → REST snapshot refresh
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
			// Immediate snapshot on fallback
			invalidateAwd();
		};

		if (preferStream && typeof EventSource !== "undefined") {
			try {
				// Backend may not expose this yet; onerror falls back to poll.
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
						// ignore malformed
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
