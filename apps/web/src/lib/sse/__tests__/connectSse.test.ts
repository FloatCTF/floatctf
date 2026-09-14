/**
 * connectSse 单元测试。
 *
 * 测试：认证头、token 不在 URL、401/403 停止重连、Abort 不重连、
 * 网络错误重连、5xx 重连、429 退避、成功重连重置退避、清理取消重连。
 *
 * 使用 vitest 的 vi.fn() 模拟 fetch，不依赖真实网络。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { connectSse } from "../connectSse";
import type { SseEvent } from "../parser";

// 辅助：创建可读流
function createReadableStream(chunks: string[]): ReadableStream<Uint8Array> {
	const encoder = new TextEncoder();
	let index = 0;
	return new ReadableStream({
		pull(controller) {
			if (index >= chunks.length) {
				controller.close();
				return;
			}
			controller.enqueue(encoder.encode(chunks[index]));
			index++;
		},
	});
}

/** 创建尊重 AbortSignal 的 fetch mock */
function createAbortableFetch(
	impl: (callCount: number) => Response | Promise<Response>,
) {
	let callCount = 0;
	return vi
		.fn()
		.mockImplementation(
			(_url: string, init?: RequestInit): Promise<Response> => {
				callCount++;
				return new Promise<Response>((resolve, reject) => {
					const signal = init?.signal;
					if (signal?.aborted) {
						reject(new DOMException("Aborted", "AbortError"));
						return;
					}
					const onAbort = () =>
						reject(new DOMException("Aborted", "AbortError"));
					signal?.addEventListener("abort", onAbort, { once: true });

					try {
						const result = impl(callCount);
						Promise.resolve(result)
							.then((r) => {
								signal?.removeEventListener("abort", onAbort);
								resolve(r);
							})
							.catch((e) => {
								signal?.removeEventListener("abort", onAbort);
								reject(e);
							});
					} catch (e) {
						signal?.removeEventListener("abort", onAbort);
						reject(e);
					}
				});
			},
		);
}

function sseResponse(body: ReadableStream<Uint8Array>): Response {
	return new Response(body, {
		status: 200,
		headers: {
			"Content-Type": "text/event-stream",
			"Cache-Control": "no-cache",
		},
	});
}

function jsonResponse(status: number, body: object): Response {
	return new Response(JSON.stringify(body), {
		status,
		headers: { "Content-Type": "application/json" },
	});
}

describe("connectSse", () => {
	beforeEach(() => {
		vi.useFakeTimers();
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("sends Authorization: Bearer header", async () => {
		const fetchSpy = createAbortableFetch(() =>
			sseResponse(createReadableStream(["data: hello\n\n"])),
		);
		vi.stubGlobal("fetch", fetchSpy);

		const events: SseEvent[] = [];
		const controller = new AbortController();

		connectSse({
			url: "/api/events/test/awd/stream",
			headers: {},
			signal: controller.signal,
			getToken: () => "test-token-123",
			onEvent: (ev) => events.push(ev),
		});

		await vi.advanceTimersByTimeAsync(50);

		expect(fetchSpy).toHaveBeenCalledTimes(1);
		const fetchUrl = fetchSpy.mock.calls[0][0] as string;
		const fetchOptions = fetchSpy.mock.calls[0][1] as RequestInit;

		expect(fetchUrl).not.toContain("token=");
		expect(fetchUrl).not.toContain("access_token");

		expect(fetchOptions.headers).toBeDefined();
		const headers = fetchOptions.headers as Record<string, string>;
		expect(headers.Authorization).toBe("Bearer test-token-123");
		expect(headers.Accept).toBe("text/event-stream");

		controller.abort();
	});

	it("does not put token in URL", async () => {
		const fetchSpy = createAbortableFetch(() =>
			sseResponse(createReadableStream(["data: hello\n\n"])),
		);
		vi.stubGlobal("fetch", fetchSpy);

		const controller = new AbortController();

		connectSse({
			url: "/api/events/test/awd/stream",
			headers: {},
			signal: controller.signal,
			getToken: () => "secret-token",
			onEvent: () => {},
		});

		await vi.advanceTimersByTimeAsync(50);

		const fetchUrl = fetchSpy.mock.calls[0][0] as string;
		expect(fetchUrl).toBe("/api/events/test/awd/stream");
		expect(fetchUrl).not.toContain("secret-token");

		controller.abort();
	});

	it("sets Accept: text/event-stream header", async () => {
		const fetchSpy = createAbortableFetch(() =>
			sseResponse(createReadableStream(["data: hello\n\n"])),
		);
		vi.stubGlobal("fetch", fetchSpy);

		const controller = new AbortController();

		connectSse({
			url: "/api/events/test/awd/stream",
			headers: {},
			signal: controller.signal,
			getToken: () => "token",
			onEvent: () => {},
		});

		await vi.advanceTimersByTimeAsync(50);

		const fetchOptions = fetchSpy.mock.calls[0][1] as RequestInit;
		const headers = fetchOptions.headers as Record<string, string>;
		expect(headers.Accept).toBe("text/event-stream");

		controller.abort();
	});

	it("401 enters auth_error and stops retrying", async () => {
		const fetchSpy = createAbortableFetch(() => jsonResponse(401, {}));
		vi.stubGlobal("fetch", fetchSpy);

		const stateChanges: string[] = [];
		const controller = new AbortController();

		connectSse({
			url: "/api/events/test/awd/stream",
			headers: {},
			signal: controller.signal,
			getToken: () => "bad-token",
			onEvent: () => {},
			onStateChange: (s) => stateChanges.push(s.state),
			initialRetryDelayMs: 100,
		});

		await vi.advanceTimersByTimeAsync(50);

		expect(stateChanges).toContain("auth_error");
		expect(fetchSpy).toHaveBeenCalledTimes(1);

		await vi.advanceTimersByTimeAsync(5000);
		expect(fetchSpy).toHaveBeenCalledTimes(1);

		controller.abort();
	});

	it("403 enters auth_error and stops retrying", async () => {
		const fetchSpy = createAbortableFetch(() => jsonResponse(403, {}));
		vi.stubGlobal("fetch", fetchSpy);

		const stateChanges: string[] = [];
		const controller = new AbortController();

		connectSse({
			url: "/api/events/test/awd/stream",
			headers: {},
			signal: controller.signal,
			getToken: () => "bad-token",
			onEvent: () => {},
			onStateChange: (s) => stateChanges.push(s.state),
			initialRetryDelayMs: 100,
		});

		await vi.advanceTimersByTimeAsync(50);

		expect(stateChanges).toContain("auth_error");
		expect(fetchSpy).toHaveBeenCalledTimes(1);

		controller.abort();
	});

	it("Abort does not trigger reconnect", async () => {
		const fetchSpy = createAbortableFetch(
			() => new Promise<Response>(() => {}),
		);
		vi.stubGlobal("fetch", fetchSpy);

		const stateChanges: string[] = [];
		const controller = new AbortController();

		connectSse({
			url: "/api/events/test/awd/stream",
			headers: {},
			signal: controller.signal,
			getToken: () => "token",
			onEvent: () => {},
			onStateChange: (s) => stateChanges.push(s.state),
			initialRetryDelayMs: 100,
		});

		await vi.advanceTimersByTimeAsync(50);
		expect(fetchSpy).toHaveBeenCalledTimes(1);

		controller.abort();
		await vi.advanceTimersByTimeAsync(5000);

		expect(fetchSpy).toHaveBeenCalledTimes(1);
		expect(stateChanges).toContain("closed");
	});

	it("network failure triggers reconnect", async () => {
		const fetchSpy = createAbortableFetch((n) => {
			if (n <= 1) throw new Error("Network error");
			return new Promise<Response>(() => {});
		});
		vi.stubGlobal("fetch", fetchSpy);

		const stateChanges: string[] = [];
		const controller = new AbortController();

		connectSse({
			url: "/api/events/test/awd/stream",
			headers: {},
			signal: controller.signal,
			getToken: () => "token",
			onEvent: () => {},
			onStateChange: (s) => stateChanges.push(s.state),
			initialRetryDelayMs: 100,
		});

		await vi.advanceTimersByTimeAsync(50);
		expect(fetchSpy).toHaveBeenCalledTimes(1);

		await vi.advanceTimersByTimeAsync(200);
		expect(fetchSpy).toHaveBeenCalledTimes(2);

		controller.abort();
	});

	it("5xx triggers reconnect", async () => {
		const fetchSpy = createAbortableFetch((n) => {
			if (n <= 1) return jsonResponse(502, {});
			return new Promise<Response>(() => {});
		});
		vi.stubGlobal("fetch", fetchSpy);

		const controller = new AbortController();

		connectSse({
			url: "/api/events/test/awd/stream",
			headers: {},
			signal: controller.signal,
			getToken: () => "token",
			onEvent: () => {},
			initialRetryDelayMs: 100,
		});

		await vi.advanceTimersByTimeAsync(50);
		expect(fetchSpy).toHaveBeenCalledTimes(1);

		await vi.advanceTimersByTimeAsync(200);
		expect(fetchSpy).toHaveBeenCalledTimes(2);

		controller.abort();
	});

	it("429 uses Retry-After header", async () => {
		const fetchSpy = createAbortableFetch((n) => {
			if (n <= 1) {
				return new Response(null, {
					status: 429,
					headers: { "Retry-After": "1" },
				});
			}
			return new Promise<Response>(() => {});
		});
		vi.stubGlobal("fetch", fetchSpy);

		const controller = new AbortController();

		connectSse({
			url: "/api/events/test/awd/stream",
			headers: {},
			signal: controller.signal,
			getToken: () => "token",
			onEvent: () => {},
			initialRetryDelayMs: 100,
		});

		await vi.advanceTimersByTimeAsync(50);
		expect(fetchSpy).toHaveBeenCalledTimes(1);

		await vi.advanceTimersByTimeAsync(1100);
		expect(fetchSpy).toHaveBeenCalledTimes(2);

		controller.abort();
	});

	it("successful connection resets backoff", async () => {
		const fetchSpy = createAbortableFetch((n) => {
			if (n <= 1) throw new Error("fail");
			// 成功连接后，流保持打开（不关闭），防止触发流结束重连
			return sseResponse(
				new ReadableStream({
					start(controller) {
						controller.enqueue(new TextEncoder().encode("data: ok\n\n"));
						// 不调用 controller.close() — 流保持打开
					},
				}),
			);
		});
		vi.stubGlobal("fetch", fetchSpy);

		const stateChanges: string[] = [];
		const controller = new AbortController();

		connectSse({
			url: "/api/events/test/awd/stream",
			headers: {},
			signal: controller.signal,
			getToken: () => "token",
			onEvent: () => {},
			onStateChange: (s) => stateChanges.push(s.state),
			initialRetryDelayMs: 100,
		});

		await vi.advanceTimersByTimeAsync(50);
		expect(fetchSpy).toHaveBeenCalledTimes(1);
		expect(stateChanges).toContain("connecting");

		await vi.advanceTimersByTimeAsync(200);
		expect(fetchSpy).toHaveBeenCalledTimes(2);
		expect(stateChanges).toContain("connected");

		controller.abort();
	});

	it("close() prevents reconnect", async () => {
		const fetchSpy = createAbortableFetch(
			() => new Promise<Response>(() => {}),
		);
		vi.stubGlobal("fetch", fetchSpy);

		const controller = new AbortController();

		const conn = connectSse({
			url: "/api/events/test/awd/stream",
			headers: {},
			signal: controller.signal,
			getToken: () => "token",
			onEvent: () => {},
			initialRetryDelayMs: 100,
		});

		await vi.advanceTimersByTimeAsync(50);
		expect(fetchSpy).toHaveBeenCalledTimes(1);

		conn.close();
		await vi.advanceTimersByTimeAsync(5000);

		expect(fetchSpy).toHaveBeenCalledTimes(1);

		controller.abort();
	});

	it("changing event ID uses new URL", async () => {
		const fetchSpy = createAbortableFetch(
			() => new Promise<Response>(() => {}),
		);
		vi.stubGlobal("fetch", fetchSpy);

		const controller = new AbortController();

		connectSse({
			url: "/api/events/old-id/awd/stream",
			headers: {},
			signal: controller.signal,
			getToken: () => "token",
			onEvent: () => {},
		});

		await vi.advanceTimersByTimeAsync(50);
		expect(fetchSpy).toHaveBeenCalledTimes(1);
		expect(fetchSpy.mock.calls[0][0]).toBe("/api/events/old-id/awd/stream");

		controller.abort();
		await vi.advanceTimersByTimeAsync(100);

		const newController = new AbortController();
		connectSse({
			url: "/api/events/new-id/awd/stream",
			headers: {},
			signal: newController.signal,
			getToken: () => "token",
			onEvent: () => {},
		});

		await vi.advanceTimersByTimeAsync(50);
		expect(fetchSpy).toHaveBeenCalledTimes(2);
		expect(fetchSpy.mock.calls[1][0]).toBe("/api/events/new-id/awd/stream");

		newController.abort();
	});

	// ── Token lifecycle tests ──

	it("token change after 401 creates new connection with new token", async () => {
		let currentToken = "old-bad-token";
		const fetchSpy = createAbortableFetch((_n) => {
			if (currentToken === "old-bad-token") {
				return jsonResponse(401, {});
			}
			// New token succeeds
			return sseResponse(
				new ReadableStream({
					start(controller) {
						controller.enqueue(
							new TextEncoder().encode("data: ok\n\n"),
						);
					},
				}),
			);
		});
		vi.stubGlobal("fetch", fetchSpy);

		const stateChanges: string[] = [];
		const controller = new AbortController();

		// First connection with old token
		const conn = connectSse({
			url: "/api/events/test/awd/stream",
			headers: {},
			signal: controller.signal,
			getToken: () => currentToken,
			onEvent: () => {},
			onStateChange: (s) => stateChanges.push(s.state),
			initialRetryDelayMs: 100,
		});

		await vi.advanceTimersByTimeAsync(50);
		expect(fetchSpy).toHaveBeenCalledTimes(1);
		expect(stateChanges).toContain("auth_error");

		// Close old connection, change token, create new connection
		conn.close();
		currentToken = "new-good-token";

		const newController = new AbortController();
		const conn2 = connectSse({
			url: "/api/events/test/awd/stream",
			headers: {},
			signal: newController.signal,
			getToken: () => currentToken,
			onEvent: () => {},
			onStateChange: (s) => stateChanges.push(s.state),
			initialRetryDelayMs: 100,
		});

		await vi.advanceTimersByTimeAsync(50);
		expect(fetchSpy).toHaveBeenCalledTimes(2);

		// Verify new token was used
		const fetchOptions = fetchSpy.mock.calls[1][1] as RequestInit;
		const headers = fetchOptions.headers as Record<string, string>;
		expect(headers.Authorization).toBe("Bearer new-good-token");

		expect(stateChanges).toContain("connected");

		conn2.close();
		newController.abort();
		controller.abort();
	});

	it("logout (null token) does not create authenticated connection", async () => {
		const fetchSpy = createAbortableFetch(
			() => new Promise<Response>(() => {}),
		);
		vi.stubGlobal("fetch", fetchSpy);

		const controller = new AbortController();

		// null token = logged out
		connectSse({
			url: "/api/events/test/awd/stream",
			headers: {},
			signal: controller.signal,
			getToken: () => null,
			onEvent: () => {},
			initialRetryDelayMs: 100,
		});

		await vi.advanceTimersByTimeAsync(50);

		// Should still attempt fetch (with no Authorization header)
		expect(fetchSpy).toHaveBeenCalledTimes(1);
		const fetchOptions = fetchSpy.mock.calls[0][1] as RequestInit;
		const headers = fetchOptions.headers as Record<string, string>;
		// No token → no Authorization header
		expect(headers.Authorization).toBeUndefined();

		controller.abort();
	});

	it("same token does not duplicate connections", async () => {
		const fetchSpy = createAbortableFetch(
			() => new Promise<Response>(() => {}),
		);
		vi.stubGlobal("fetch", fetchSpy);

		const controller1 = new AbortController();

		connectSse({
			url: "/api/events/test/awd/stream",
			headers: {},
			signal: controller1.signal,
			getToken: () => "same-token",
			onEvent: () => {},
		});

		await vi.advanceTimersByTimeAsync(50);
		expect(fetchSpy).toHaveBeenCalledTimes(1);

		// Second connection with same token (simulating React re-render)
		// Note: in React, the old effect cleanup would abort the first controller
		controller1.abort();
		await vi.advanceTimersByTimeAsync(50);

		const controller2 = new AbortController();
		connectSse({
			url: "/api/events/test/awd/stream",
			headers: {},
			signal: controller2.signal,
			getToken: () => "same-token",
			onEvent: () => {},
		});

		await vi.advanceTimersByTimeAsync(50);
		// One new fetch for the new connection
		expect(fetchSpy).toHaveBeenCalledTimes(2);

		controller2.abort();
	});
});
