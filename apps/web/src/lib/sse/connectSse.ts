/**
 * 浏览器认证的 fetch-based SSE 客户端。
 *
 * 原生 EventSource 无法发送 Authorization 等自定义请求头，
 * 因此使用 fetch() + ReadableStream 实现 SSE 传输。
 *
 * 特性：
 * - Bearer token 通过 Authorization 头传递（永不进入 URL）
 * - AbortController 生命周期管理
 * - 自动重连（指数退避 + 抖动）
 * - 认证失败 (401/403) 停止重连
 * - 连接状态暴露
 * - Last-Event-ID 支持（若后端提供）
 */

import { type SseEvent, type SseParser, createSseParser } from "./parser";

// ── 类型 ────────────────────────────────────────────────────────────────────

export type SseConnectionState =
	| "idle"
	| "connecting"
	| "connected"
	| "reconnecting"
	| "auth_error"
	| "error"
	| "closed";

export interface SseConnectionStatus {
	state: SseConnectionState;
	lastEventAt: Date | null;
	lastError: Error | null;
	retryCount: number;
}

export interface ConnectSseOptions {
	/** SSE 端点完整 URL */
	url: string;
	/** 请求头（含 Authorization: Bearer <token>） */
	headers: Record<string, string>;
	/** 外部分发的 AbortSignal（用于 React 清理） */
	signal: AbortSignal;
	/** 上次接收的事件 ID（用于 Last-Event-ID 重连） */
	lastEventId?: string;
	/** 连接成功回调 */
	onOpen?: () => void;
	/** 事件回调 */
	onEvent: (event: SseEvent) => void;
	/** 错误回调 */
	onError?: (error: Error) => void;
	/** 连接状态变化回调 */
	onStateChange?: (status: SseConnectionStatus) => void;
	/** 初始重连延迟（毫秒），默认 1000 */
	initialRetryDelayMs?: number;
	/** 最大重连延迟（毫秒），默认 30000 */
	maxRetryDelayMs?: number;
	/** 从响应中获取 token 的函数（用于 token 刷新场景） */
	getToken?: () => string | null;
}

export interface SseConnection {
	/** 主动关闭连接 */
	close(): void;
	/** 当前连接状态 */
	readonly status: SseConnectionStatus;
}

// ── 默认值 ──────────────────────────────────────────────────────────────────

const DEFAULT_INITIAL_RETRY_MS = 1000;
const DEFAULT_MAX_RETRY_MS = 30000;
const JITTER_FACTOR = 0.3;

// ── 实现 ────────────────────────────────────────────────────────────────────

export function connectSse(options: ConnectSseOptions): SseConnection {
	const {
		url,
		headers: initialHeaders,
		signal,
		lastEventId,
		onOpen,
		onEvent,
		onError,
		onStateChange,
		initialRetryDelayMs = DEFAULT_INITIAL_RETRY_MS,
		maxRetryDelayMs = DEFAULT_MAX_RETRY_MS,
		getToken,
	} = options;

	let abortController: AbortController | null = null;
	let retryTimer: ReturnType<typeof setTimeout> | null = null;
	let retryCount = 0;
	let currentDelay = initialRetryDelayMs;
	let connected = false;
	let disposed = false;

	const status: SseConnectionStatus = {
		state: "idle",
		lastEventAt: null,
		lastError: null,
		retryCount: 0,
	};

	function setState(state: SseConnectionState, error?: Error) {
		status.state = state;
		status.retryCount = retryCount;
		if (error) {
			status.lastError = error;
		}
		onStateChange?.({ ...status });
	}

	function jitter(delay: number): number {
		const factor = 1 + (Math.random() * 2 - 1) * JITTER_FACTOR;
		return Math.round(delay * factor);
	}

	function buildHeaders(): Record<string, string> {
		const h: Record<string, string> = {
			Accept: "text/event-stream",
			...initialHeaders,
		};
		// 刷新 token（若提供 getToken）
		if (getToken) {
			const freshToken = getToken();
			if (freshToken) {
				h.Authorization = `Bearer ${freshToken}`;
			}
		}
		if (lastEventId) {
			h["Last-Event-ID"] = lastEventId;
		}
		return h;
	}

	async function connect(currentLastEventId?: string) {
		if (disposed || signal.aborted) return;

		// 取消上一次的 AbortController
		abortController?.abort();
		abortController = new AbortController();

		// 合并外部 signal 和内部 abort
		const combinedSignal = abortController.signal;
		const onExternalAbort = () => abortController?.abort();
		signal.addEventListener("abort", onExternalAbort, { once: true });

		setState(connected ? "reconnecting" : "connecting");

		try {
			const headers = buildHeaders();
			if (currentLastEventId) {
				headers["Last-Event-ID"] = currentLastEventId;
			}

			const response = await fetch(url, {
				method: "GET",
				headers,
				signal: combinedSignal,
			});

			// 认证失败 → 不重连
			if (response.status === 401 || response.status === 403) {
				setState("auth_error", new Error(`HTTP ${response.status}`));
				onError?.(new Error(`SSE auth failed: HTTP ${response.status}`));
				return;
			}

			// 客户端错误 (4xx 非 401/403/429) → 致命
			if (
				response.status >= 400 &&
				response.status < 500 &&
				response.status !== 429
			) {
				setState("error", new Error(`HTTP ${response.status}`));
				onError?.(new Error(`SSE fatal: HTTP ${response.status}`));
				return;
			}

			if (!response.ok && response.status !== 429) {
				throw new Error(`HTTP ${response.status}`);
			}

			// 429 → 使用 Retry-After 或退避
			if (response.status === 429) {
				const retryAfter = response.headers.get("Retry-After");
				const delay = retryAfter
					? Number.parseInt(retryAfter, 10) * 1000
					: jitter(currentDelay);
				scheduleReconnect(delay);
				return;
			}

			// 验证 Content-Type
			const contentType = response.headers.get("Content-Type") ?? "";
			if (!contentType.includes("text/event-stream")) {
				setState("error", new Error(`Unexpected Content-Type: ${contentType}`));
				onError?.(new Error(`SSE: unexpected Content-Type: ${contentType}`));
				return;
			}

			if (!response.body) {
				throw new Error("No response body");
			}

			// ── 连接成功 ──
			connected = true;
			retryCount = 0;
			currentDelay = initialRetryDelayMs;
			setState("connected");
			onOpen?.();

			// 读取流
			const reader = response.body.getReader();
			const parser: SseParser = createSseParser();

			try {
				while (true) {
					const { done, value } = await reader.read();
					if (done) break;

					const events = parser.push(value);
					for (const ev of events) {
						status.lastEventAt = new Date();
						onEvent(ev);
					}
				}
			} finally {
				reader.releaseLock();
			}

			// 流正常结束 → 重连
			if (!disposed && !signal.aborted) {
				scheduleReconnect(jitter(currentDelay));
			}
		} catch (err: unknown) {
			const error = err instanceof Error ? err : new Error(String(err));

			// AbortError → 正常关闭，不重连
			if (error.name === "AbortError") {
				setState("closed");
				return;
			}

			// 已释放或外部信号已中止
			if (disposed || signal.aborted) return;

			// 网络错误 → 重连
			status.lastError = error;
			onError?.(error);
			scheduleReconnect(jitter(currentDelay));
		} finally {
			signal.removeEventListener("abort", onExternalAbort);
		}
	}

	function scheduleReconnect(delay: number) {
		if (disposed || signal.aborted) return;

		retryCount++;
		// 指数退避
		currentDelay = Math.min(currentDelay * 2, maxRetryDelayMs);
		const actualDelay = Math.min(delay, currentDelay);

		setState("reconnecting");

		retryTimer = setTimeout(() => {
			retryTimer = null;
			connect();
		}, actualDelay);
	}

	function close() {
		disposed = true;
		abortController?.abort();
		if (retryTimer) {
			clearTimeout(retryTimer);
			retryTimer = null;
		}
		setState("closed");
	}

	// 启动连接
	connect(lastEventId);

	return {
		close,
		get status() {
			return { ...status };
		},
	};
}
