import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
// @vitest-environment jsdom
/**
 * AwdpEventProgress 测试：赛事顶部进度条（route 标题与导航之间）。
 *  - Break：countdown 文案 + Break/Fix 双段 + 竖线分隔
 *  - Fix：Next check in 文案 + Turn 文案
 *  - 无 overview 数据（未加入）→ 不渲染
 */
import { cleanup, render, screen } from "@testing-library/react";
import {
	afterEach,
	beforeAll,
	beforeEach,
	describe,
	expect,
	it,
	vi,
} from "vitest";

import type { AwdpOverview } from "@/api/awdp";

import { AwdpEventProgress } from "../AwdpEventProgress";

beforeAll(() => {
	Object.defineProperty(window, "matchMedia", {
		writable: true,
		value: vi.fn().mockImplementation((query: string) => ({
			matches: false,
			media: query,
			onchange: null,
			addListener: vi.fn(),
			removeListener: vi.fn(),
			addEventListener: vi.fn(),
			removeEventListener: vi.fn(),
			dispatchEvent: vi.fn(),
		})),
	});
	// 固定系统时钟：组件内部用 Date.now() 每秒刷新。
	vi.useFakeTimers();
	vi.setSystemTime(new Date(now));
});

beforeEach(() => {
	vi.setSystemTime(new Date(now));
});

afterEach(cleanup);

const now = Date.parse("2026-08-14T08:00:00Z");

function makeOverview(overrides: Partial<AwdpOverview> = {}): AwdpOverview {
	return {
		event_id: "evt-1",
		phase: "break",
		break_duration_secs: 1800,
		fix_duration_secs: 5400,
		fix_round_interval_secs: 900,
		total_rounds: 6,
		break_score: 1000,
		fix_round_score: 150,
		started_at: new Date(now - 900_000).toISOString(),
		break_ends_at: new Date(now + 900_000).toISOString(),
		fix_started_at: null,
		fix_ends_at: null,
		finished_at: null,
		current_round: 0,
		next_action_at: null,
		my_score: 0,
		gameboxes: [],
		...overrides,
	};
}

function renderWithQuery(id: string, data: AwdpOverview | null) {
	const queryClient = new QueryClient({
		defaultOptions: {
			queries: {
				retry: false,
				enabled: false, // 用 initialData 注入，避免真实 fetch
			},
		},
	});
	queryClient.setQueryData(["awdp-overview", id], {
		data,
	});
	return render(
		<QueryClientProvider client={queryClient}>
			<AwdpEventProgress id={id} />
		</QueryClientProvider>,
	);
}

describe("AwdpEventProgress", () => {
	it("Break：Ends in 文案 + Break/Fix 分段 progressbar", () => {
		renderWithQuery("evt-1", makeOverview());
		expect(screen.getByText(/Ends in: 15m/)).toBeDefined();
		const bar = screen.getByRole("progressbar");
		expect(bar).toBeDefined();
		expect(bar.getAttribute("aria-valuemin")).toBe("0");
		expect(bar.getAttribute("aria-valuemax")).toBe("100");
		// 段名
		expect(screen.getByText("Break")).toBeDefined();
		expect(screen.getByText("Fix")).toBeDefined();
	});

	it("Fix：Next check in 文案 + Turn 进度", () => {
		renderWithQuery(
			"evt-1",
			makeOverview({
				phase: "fix",
				current_round: 3,
				started_at: new Date(now - 3600_000).toISOString(),
				fix_started_at: new Date(now - 300_000).toISOString(),
				next_action_at: new Date(now + 300_000).toISOString(),
			}),
		);
		expect(screen.getByText(/Next check in: 5m/)).toBeDefined();
		expect(screen.getByText("Turn 3 / 6")).toBeDefined();
	});

	it("无 overview（未加入/未开始）→ 不渲染", () => {
		renderWithQuery("evt-1", null);
		expect(screen.queryByRole("progressbar")).toBeNull();
	});

	it("ended：Finished 文案", () => {
		renderWithQuery(
			"evt-1",
			makeOverview({
				phase: "ended",
				current_round: 6,
				started_at: new Date(now - 7200_000).toISOString(),
				fix_started_at: new Date(now - 6300_000).toISOString(),
			}),
		);
		expect(screen.getByText("Finished")).toBeDefined();
	});
});
