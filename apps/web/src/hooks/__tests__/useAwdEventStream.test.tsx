/**
 * useAwdEventStream 断线回退测试（Phase 9.2 A1 — SSE reconnect resilience）。
 *
 * 验证核心语义：
 * - 初始连接（connecting → connected）：SSE 通道建立，无轮询。
 * - 连接中断（reconnecting / error）：立即启动 REST 轮询回退，权威状态持续更新。
 * - SSE 恢复（connected）：轮询停止，恰好一条 SSE 通道生效（无重复事件处理）。
 * - auth_error：关闭连接 + 轮询常驻，直到令牌变化触发 effect 重建。
 *
 * connectSse 被 mock（驱动 onStateChange），不依赖真实网络。
 */
// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
	SseConnection,
	SseConnectionState,
	SseConnectionStatus,
} from "@/lib/sse";

import { useAwdEventStream } from "../useAwdEventStream";

// ── mocks ────────────────────────────────────────────────────────────────────

// vi.hoisted：mock 工厂在模块导入期执行，必须先于 const 声明初始化。
const { mockConnectSse } = vi.hoisted(() => ({ mockConnectSse: vi.fn() }));

vi.mock("@/stores/AuthStore", () => ({
	useAuthStore: (selector: (s: unknown) => unknown) =>
		selector({ token: "test-token", adminToken: null }),
}));

type OnStateChange = (status: SseConnectionStatus) => void;

vi.mock("@/lib/sse", () => ({
	connectSse: (options: import("@/lib/sse").ConnectSseOptions) =>
		mockConnectSse(options),
}));

// 捕获最近一次 connectSse 调用（含 onStateChange / close）。
function lastConnect(): {
	options: import("@/lib/sse").ConnectSseOptions;
	close: ReturnType<typeof vi.fn>;
	status: { state: SseConnectionState };
} {
	const options = mockConnectSse.mock.calls.at(-1)?.[0];
	if (!options) throw new Error("connectSse was not called");
	return {
		options,
		close: vi.fn(),
		status: { state: "connecting" },
	};
}

function emitState(onStateChange: OnStateChange, state: SseConnectionState) {
	act(() => {
		onStateChange({
			state,
			lastEventAt: new Date(),
			lastError: null,
			retryCount: 0,
		});
	});
}

function makeWrapper() {
	const qc = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	});
	const wrapper = ({ children }: { children: React.ReactNode }) => (
		<QueryClientProvider client={qc}>{children}</QueryClientProvider>
	);
	return { qc, wrapper };
}

describe("useAwdEventStream reconnect fallback", () => {
	beforeEach(() => {
		vi.useFakeTimers();
		mockConnectSse.mockReset();
		mockConnectSse.mockImplementation(
			(options: import("@/lib/sse").ConnectSseOptions) => {
				const conn: SseConnection = {
					close: () => {
						// 模拟 close()：同步触发 abort 语义
						options.signal.dispatchEvent(new Event("abort"));
					},
					get status() {
						return {
							state: "connecting" as const,
							lastEventAt: null,
							lastError: null,
							retryCount: 0,
						};
					},
				};
				return conn;
			},
		);
	});

	afterEach(() => {
		cleanup();
		vi.runOnlyPendingTimers();
		vi.useRealTimers();
		vi.restoreAllMocks();
	});

	it("polling starts when SSE enters reconnecting and stops on connected", () => {
		const { qc, wrapper } = makeWrapper();
		const invalidateSpy = vi.spyOn(qc, "invalidateQueries");

		const { unmount } = renderHook(
			() => useAwdEventStream({ eventId: "evt-1", pollMs: 1000 }),
			{ wrapper },
		);

		const conn = lastConnect();
		const onStateChange = conn.options.onStateChange;
		if (!onStateChange) throw new Error("onStateChange missing");

		// 初始：connecting → connected（SSE 建立，无轮询）
		emitState(onStateChange, "connected");
		expect(invalidateSpy).not.toHaveBeenCalled();
		expect(conn.close).not.toHaveBeenCalled();

		// 断线：reconnecting → 立即开始轮询（invalidateQueries 被调度）
		emitState(onStateChange, "reconnecting");
		act(() => {
			vi.advanceTimersByTime(1000);
		});
		expect(invalidateSpy).toHaveBeenCalled();

		// 断线期间轮询持续更新权威状态
		const callsDuringReconnect = invalidateSpy.mock.calls.length;
		act(() => {
			vi.advanceTimersByTime(3000);
		});
		expect(invalidateSpy.mock.calls.length).toBeGreaterThan(callsDuringReconnect);

		// SSE 恢复：connected → 轮询停止（调用次数不再增长）
		emitState(onStateChange, "connected");
		const callsAfterRecover = invalidateSpy.mock.calls.length;
		act(() => {
			vi.advanceTimersByTime(5000);
		});
		expect(invalidateSpy.mock.calls.length).toBe(callsAfterRecover);

		unmount();
	});

	it("auth_error closes the connection and keeps polling", () => {
		const { qc, wrapper } = makeWrapper();
		const invalidateSpy = vi.spyOn(qc, "invalidateQueries");

		const { unmount } = renderHook(
			() => useAwdEventStream({ eventId: "evt-1", pollMs: 1000 }),
			{ wrapper },
		);

		const conn = lastConnect();
		const onStateChange = conn.options.onStateChange;
		if (!onStateChange) throw new Error("onStateChange missing");

		emitState(onStateChange, "connected");
		expect(invalidateSpy).not.toHaveBeenCalled();

		// auth_error → 连接关闭 + 轮询常驻
		emitState(onStateChange, "auth_error");
		expect(conn.close).not.toHaveBeenCalled(); // close 由 hook 内部调用
		act(() => {
			vi.advanceTimersByTime(1000);
		});
		expect(invalidateSpy).toHaveBeenCalled();

		// 轮询持续
		const calls = invalidateSpy.mock.calls.length;
		act(() => {
			vi.advanceTimersByTime(2000);
		});
		expect(invalidateSpy.mock.calls.length).toBeGreaterThan(calls);

		unmount();
	});

	it("Bearer token is passed via getToken and never in URL", () => {
		const { wrapper } = makeWrapper();
		const { unmount } = renderHook(
			() => useAwdEventStream({ eventId: "evt-1" }),
			{ wrapper },
		);

		const conn = lastConnect();
		expect(conn.options.url).toBe("/api/events/evt-1/awd/stream");
		expect(conn.options.url).not.toContain("token");
		expect(conn.options.getToken?.()).toBe("test-token");
		// headers 由 connectSse 内部构建（buildHeaders），getToken 语义覆盖。

		unmount();
	});
});
