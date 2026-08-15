import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
// @vitest-environment jsdom
/**
 * AwdpEventProgress 测试：赛事顶部进度条（route 标题与导航之间）。
 *  - Break：countdown 文案 + Break/Fix 双段 + 竖线分隔
 *  - Fix：Next check in 文案 + Turn 文案
 *  - 无 overview 数据（未加入）→ 不渲染
 */
import { act, cleanup, render, screen } from "@testing-library/react";
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
import { awdpPlayerApi } from "@/api/awdp";

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

afterEach(() => {
	cleanup();
	vi.restoreAllMocks();
});

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

function renderWithQuery(
	id: string,
	data: AwdpOverview | null,
	event?: { start_time: string; end_time?: string | null },
) {
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
	queryClient.setQueryData(["eventInfo", id], {
		data: {
			event: event ?? { start_time: "" },
			joined: true,
			team_result: null,
		},
	});
	return render(
		<QueryClientProvider client={queryClient}>
			<AwdpEventProgress id={id} />
		</QueryClientProvider>,
	);
}

describe("AwdpEventProgress", () => {
	it("Break：阶段名 + 剩余时间文案（Break 15m）+ Break/Fix 分段 progressbar", () => {
		renderWithQuery("evt-1", makeOverview());
		expect(screen.getByText(/Break 15m/)).toBeDefined();
		const bar = screen.getByRole("progressbar");
		expect(bar).toBeDefined();
		expect(bar.getAttribute("aria-valuemin")).toBe("0");
		expect(bar.getAttribute("aria-valuemax")).toBe("100");
		// 段名
		expect(screen.getByText("Break")).toBeDefined();
		expect(screen.getByText("Fix")).toBeDefined();
	});

	it("Fix：阶段名 + 下一检查文案（Fix 5m）+ Turn 进度", () => {
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
		expect(screen.getByText(/Fix 5m/)).toBeDefined();
		expect(screen.getByText("Turn 3 / 6")).toBeDefined();
	});

	it("pending：显示还有多久开始（Starts in）", () => {
		renderWithQuery(
			"evt-1",
			makeOverview({
				phase: "pending",
				started_at: null,
				break_ends_at: null,
				next_action_at: null,
			}),
			{ start_time: new Date(now + 7200_000).toISOString() },
		);
		expect(screen.getByText(/Starts in: 2h/)).toBeDefined();
	});

	it("pending + start 已过 + 赛事已结束 → Ended（回归：不再 Starts in: 0s）", () => {
		renderWithQuery(
			"evt-1",
			makeOverview({
				phase: "pending",
				started_at: null,
				break_ends_at: null,
				next_action_at: null,
			}),
			{
				start_time: new Date(now - 7200_000).toISOString(),
				end_time: new Date(now - 3600_000).toISOString(),
			},
		);
		expect(screen.getByText("Ended")).toBeDefined();
		expect(screen.queryByText(/Starts in/)).toBeNull();
	});

	it("pending + start 已过 + 赛事进行中（未结束/无 end_time）→ Waiting to start", () => {
		renderWithQuery(
			"evt-1",
			makeOverview({
				phase: "pending",
				started_at: null,
				break_ends_at: null,
				next_action_at: null,
			}),
			{
				start_time: new Date(now - 7200_000).toISOString(),
				end_time: new Date(now + 3600_000).toISOString(),
			},
		);
		expect(screen.getByText("Waiting to start")).toBeDefined();
		expect(screen.queryByText(/Starts in/)).toBeNull();
	});

	it("pending + start 已过 + 无 end_time（练习/开放式）→ Waiting to start", () => {
		renderWithQuery(
			"evt-1",
			makeOverview({
				phase: "pending",
				started_at: null,
				break_ends_at: null,
				next_action_at: null,
			}),
			{ start_time: new Date(now - 7200_000).toISOString() },
		);
		expect(screen.getByText("Waiting to start")).toBeDefined();
		expect(screen.queryByText(/Starts in/)).toBeNull();
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

	it("preparing_fix：过渡提示 Preparing Fix…", () => {
		renderWithQuery(
			"evt-1",
			makeOverview({
				phase: "preparing_fix",
				break_ends_at: null,
				next_action_at: null,
			}),
		);
		expect(screen.getByText("Preparing Fix…")).toBeDefined();
	});

	it("倒计时到点：越过 deadline 的瞬间立即重新获取数据（不依赖 SSE/15s poll）", async () => {
		const spy = vi
			.spyOn(awdpPlayerApi, "overview")
			.mockResolvedValue({ code: 0, message: "ok", data: makeOverview() });
		const queryClient = new QueryClient({
			defaultOptions: { queries: { retry: false } },
		});
		render(
			<QueryClientProvider client={queryClient}>
				<AwdpEventProgress id="evt-1" />
			</QueryClientProvider>,
		);
		// 初始 fetch。
		expect(spy).toHaveBeenCalledTimes(1);
		// 未到点（break_ends_at = now+900s）：推进 800s 不应额外请求。
		await act(async () => {
			vi.advanceTimersByTime(800_000);
		});
		expect(spy).toHaveBeenCalledTimes(1);
		// 越过 break_ends_at：1s tick 检测到到点 → 立即 refetch。
		await act(async () => {
			vi.advanceTimersByTime(200_000);
		});
		expect(spy.mock.calls.length).toBeGreaterThanOrEqual(2);
	});

	it("到点后 refetch 反映新阶段 → 停止高频刷新", async () => {
		let calls = 0;
		const spy = vi
			.spyOn(awdpPlayerApi, "overview")
			.mockImplementation(async () => {
				calls += 1;
				// 第二次起返回 fix（阶段已切换）→ deadline 变为下一轮 cutoff（未来）。
				if (calls >= 2) {
					return {
						code: 0,
						message: "ok",
						data: makeOverview({
							phase: "fix",
							current_round: 1,
							started_at: new Date(now - 1800_000).toISOString(),
							fix_started_at: new Date(now).toISOString(),
							next_action_at: new Date(now + 3600_000).toISOString(),
						}),
					};
				}
				return { code: 0, message: "ok", data: makeOverview() };
			});
		const queryClient = new QueryClient({
			defaultOptions: { queries: { retry: false } },
		});
		render(
			<QueryClientProvider client={queryClient}>
				<AwdpEventProgress id="evt-1" />
			</QueryClientProvider>,
		);
		expect(spy).toHaveBeenCalledTimes(1);
		// 越过 break_ends_at → 到点立即 refetch（返回 fix）。
		await act(async () => {
			vi.advanceTimersByTime(901_000);
		});
		expect(spy.mock.calls.length).toBeGreaterThanOrEqual(2);
		// 等 data 更新稳定后（refetch 反映 fix 阶段）：兜底 interval 停止高频刷新。
		await act(async () => {
			vi.advanceTimersByTime(30_000);
		});
		const settled = spy.mock.calls.length;
		// fix 阶段 deadline（next_action_at = now+1h）未到：再推进 60s 不应新增请求。
		await act(async () => {
			vi.advanceTimersByTime(60_000);
		});
		expect(spy.mock.calls.length).toBe(settled);
	});

	it("pending + start 早已过期（卡死无 run）：补抓一次后停止 2s 高频轮询", async () => {
		const spy = vi
			.spyOn(awdpPlayerApi, "overview")
			.mockResolvedValue({
				code: 0,
				message: "ok",
				data: makeOverview({
					phase: "pending",
					started_at: null,
					break_ends_at: null,
					next_action_at: null,
				}),
			});
		const queryClient = new QueryClient({
			defaultOptions: { queries: { retry: false } },
		});
		queryClient.setQueryData(["eventInfo", "evt-1"], {
			data: {
				event: {
					start_time: new Date(now - 7200_000).toISOString(),
					end_time: new Date(now + 3600_000).toISOString(),
				},
				joined: true,
				team_result: null,
			},
		});
		render(
			<QueryClientProvider client={queryClient}>
				<AwdpEventProgress id="evt-1" />
			</QueryClientProvider>,
		);
		// 初始 fetch。
		expect(spy).toHaveBeenCalledTimes(1);
		// 挂载后 overview 就绪 + 1s tick → 检测到 deadline 早已过期 → 补抓一次。
		await act(async () => {
			vi.advanceTimersByTime(1000);
		});
		expect(spy.mock.calls.length).toBeGreaterThanOrEqual(2);
		const settled = spy.mock.calls.length;
		// 之后 2s 兜底轮询因 deadline 已过期超 60s 停止：推进 30s 不应新增请求。
		await act(async () => {
			vi.advanceTimersByTime(30_000);
		});
		expect(spy.mock.calls.length).toBe(settled);
	});
});
