/**
 * React SSE hook 的 base URL 契约测试（Phase 12.1 / P0-B）。
 *
 * 背景：这些 hook 曾经硬编码 `/api/...`，于是「REST 走配置的 API、SSE 却打前端自己的
 * 源」——外部 React 前端用非默认 base URL（例如 dev server 跨源指向
 * `http://127.0.0.1:17780/api`）时实时通道直接连错地方。
 *
 * 这里**不 mock connectSse**：让真实传输跑起来，只 mock 全局 fetch，
 * 从而断言最终真正被请求的绝对 URL 与 Authorization 头。
 */
// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { createFloatCTFClient } from "@floatctf/sdk";

import { createUseAdminAwdEventStream } from "../useAdminAwdEventStream";
import { createUseAwdEventStream } from "../useAwdEventStream";
import { createUseAwdpEventStream } from "../useAwdpEventStream";
import { createUseAwdpRunStream } from "../useAwdpRunStream";

const API_BASE = "http://127.0.0.1:17780/api";

const calls: { url: string; authorization?: string }[] = [];

/** 一个永不结束的 SSE 响应体：连接保持打开，便于断言请求本身。 */
function streamingResponse(): Response {
	const body = new ReadableStream<Uint8Array>({
		start(controller) {
			controller.enqueue(new TextEncoder().encode(": keep-alive\n\n"));
		},
	});
	return new Response(body, {
		status: 200,
		headers: { "Content-Type": "text/event-stream" },
	});
}

function makeClient() {
	return createFloatCTFClient({
		baseUrl: API_BASE,
		getUserToken: () => "USER_TOKEN",
		getAdminToken: () => "ADMIN_TOKEN",
	});
}

function makeWrapper() {
	const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
	return ({ children }: { children: React.ReactNode }) => (
		<QueryClientProvider client={qc}>{children}</QueryClientProvider>
	);
}

describe("React SSE hooks honour the supplied client base URL", () => {
	beforeEach(() => {
		calls.length = 0;
		vi.stubGlobal(
			"fetch",
			vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
				const headers = (init?.headers ?? {}) as Record<string, string>;
				calls.push({
					url: String(input),
					authorization: headers.Authorization,
				});
				return streamingResponse();
			}),
		);
	});

	afterEach(() => {
		cleanup();
		vi.unstubAllGlobals();
		vi.restoreAllMocks();
	});

	it("AWD player stream targets the client's player base URL", () => {
		const client = makeClient();
		const useStream = createUseAwdEventStream(client, () => "USER_TOKEN");
		const { unmount } = renderHook(
			() => useStream({ eventId: "evt-1" }),
			{ wrapper: makeWrapper() },
		);
		expect(calls[0]?.url).toBe(`${API_BASE}/events/evt-1/awd/stream`);
		expect(calls[0]?.authorization).toBe("Bearer USER_TOKEN");
		unmount();
	});

	it("AWD admin stream targets the client's admin base URL with the admin token", () => {
		const client = makeClient();
		const useStream = createUseAdminAwdEventStream(client, () => "ADMIN_TOKEN");
		const { unmount } = renderHook(
			() => useStream({ eventId: "evt-2" }),
			{ wrapper: makeWrapper() },
		);
		expect(calls[0]?.url).toBe(`${API_BASE}/admin/events/evt-2/awd/stream`);
		expect(calls[0]?.authorization).toBe("Bearer ADMIN_TOKEN");
		unmount();
	});

	it("AWDP event stream targets the client's player base URL", () => {
		const client = makeClient();
		const useStream = createUseAwdpEventStream(client, () => "USER_TOKEN");
		const { unmount } = renderHook(
			() => useStream({ eventId: "evt-3" }),
			{ wrapper: makeWrapper() },
		);
		expect(calls[0]?.url).toBe(`${API_BASE}/events/evt-3/awdp/stream`);
		expect(calls[0]?.authorization).toBe("Bearer USER_TOKEN");
		unmount();
	});

	it("AWDP run stream targets the client's player base URL", () => {
		const client = makeClient();
		const useStream = createUseAwdpRunStream(client, () => "USER_TOKEN");
		const { unmount } = renderHook(
			() => useStream({ runId: "run-9" }),
			{ wrapper: makeWrapper() },
		);
		expect(calls[0]?.url).toBe(`${API_BASE}/service/awdp/runs/run-9/stream`);
		expect(calls[0]?.authorization).toBe("Bearer USER_TOKEN");
		unmount();
	});

	it("never hardcodes /api when the client uses a different origin", () => {
		const client = createFloatCTFClient({
			baseUrl: "https://ctf.example/api",
			getUserToken: () => "USER_TOKEN",
		});
		const useStream = createUseAwdEventStream(client, () => "USER_TOKEN");
		const { unmount } = renderHook(
			() => useStream({ eventId: "evt-4" }),
			{ wrapper: makeWrapper() },
		);
		expect(calls[0]?.url).toBe("https://ctf.example/api/events/evt-4/awd/stream");
		unmount();
	});
});
