// @vitest-environment jsdom
/**
 * AwdpScoreboard 测试：赛事 Scoreboard 双视图（SegmentedControl）。
 *  - User view（默认）：聚合排名表（Rank/参与者/Break/Fix/Total + is_me 高亮）
 *  - Gamebox view：题目视角——每题 Break 成功队伍数 / Fix 成功队伍数
 *    （Fix 成功 = 至少一轮 PATCHED，不计轮数）
 *  - 空态：无题目且无参与者 → 占位
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import type { AwdpScoreboardDetail } from "@/api/awdp";

import { AwdpScoreboardView } from "../AwdpScoreboard";

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
	// Primer experimental DataTable 需要 ResizeObserver（jsdom 缺失）。
	class ResizeObserverStub {
		observe() {}
		unobserve() {}
		disconnect() {}
	}
	globalThis.ResizeObserver = globalThis.ResizeObserver ?? ResizeObserverStub;
});

afterEach(cleanup);

const data: AwdpScoreboardDetail = {
	participant_mode: "individual",
	gameboxes: [
		{ id: "gb1", name: "web1", category: "web" },
		{ id: "gb2", name: "pwn1", category: "pwn" },
	],
	rounds: [
		{ sequence: 1, status: "completed", cutoff_at: "2026-08-14T08:15:00Z" },
		{ sequence: 2, status: "completed", cutoff_at: "2026-08-14T08:30:00Z" },
	],
	rows: [
		{
			subject_id: "u1",
			subject_name: "Alice",
			rank: 1,
			break_score: 1000,
			fix_score: 300,
			total_score: 1300,
			is_me: true,
			break_status: [true, false],
			fix_gamebox_score: [300, 0],
			fix_round_status: [
				["patched", "no_patch"],
				[null, null],
			],
		},
		{
			subject_id: "u2",
			subject_name: "Bob",
			rank: 2,
			break_score: 0,
			fix_score: 150,
			total_score: 150,
			is_me: false,
			break_status: [false, false],
			fix_gamebox_score: [150, 0],
			fix_round_status: [
				["patched", null],
				[null, null],
			],
		},
	],
};

describe("AwdpScoreboard", () => {
	it("默认 User view：聚合排名表（Rank/参与者/Break/Fix/Total + is_me）", () => {
		render(<AwdpScoreboardView data={data} />);
		expect(screen.getAllByText("Alice").length).toBeGreaterThanOrEqual(1);
		expect(screen.getAllByText("Bob").length).toBeGreaterThanOrEqual(1);
		expect(screen.getAllByText("1300")).toHaveLength(1);
		expect(screen.getAllByText("150").length).toBeGreaterThanOrEqual(1);
		// Alice 是当前用户 → me 标记
		expect(screen.getByText("me")).toBeDefined();
		// 默认不显示 Gamebox 统计
		expect(screen.queryByText("Break 成功队伍数")).toBeNull();
	});

	it("切到 Gamebox view：每题 Break/Fix 成功人数（个人赛按人数；不计轮数）", () => {
		render(<AwdpScoreboardView data={data} />);
		fireEvent.click(screen.getByRole("button", { name: /Gamebox/ }));

		expect(screen.getByText("web1")).toBeDefined();
		expect(screen.getByText("pwn1")).toBeDefined();
		// Individual 模式：文案按"人数"而非"队伍数"
		expect(screen.getByText("Break 成功人数")).toBeDefined();
		expect(screen.getByText("Fix 成功人数")).toBeDefined();
		expect(screen.queryByText("Break 成功队伍数")).toBeNull();
		// 语义说明：不计成功轮数
		expect(
			screen.getByText(/至少一轮官方 check PATCHED 的人数/),
		).toBeDefined();

		// web1：break 1（Alice）/ fix 2（Alice、Bob 均 PATCHED 过）；pwn1：0/0
		expect(screen.getAllByText("1")).toHaveLength(1);
		expect(screen.getAllByText("2")).toHaveLength(1);
		expect(screen.getAllByText("0")).toHaveLength(2);
	});

	it("团队赛 Gamebox view：文案按「队伍数」", () => {
		render(
			<AwdpScoreboardView data={{ ...data, participant_mode: "team" }} />,
		);
		fireEvent.click(screen.getByRole("button", { name: /Gamebox/ }));
		expect(screen.getByText("Break 成功队伍数")).toBeDefined();
		expect(screen.getByText("Fix 成功队伍数")).toBeDefined();
	});

	it("Gamebox view 与 User view 可来回切换", () => {
		render(<AwdpScoreboardView data={data} />);
		fireEvent.click(screen.getByRole("button", { name: /Gamebox/ }));
		expect(screen.getByText("Break 成功人数")).toBeDefined();

		fireEvent.click(screen.getByRole("button", { name: /User/ }));
		expect(screen.queryByText("Break 成功人数")).toBeNull();
		expect(screen.getByText("me")).toBeDefined();
	});

	it("无题目且无参与者 → 空态占位", () => {
		render(
			<AwdpScoreboardView
				data={{
					participant_mode: "individual",
					gameboxes: [],
					rounds: [],
					rows: [],
				}}
			/>,
		);
		expect(screen.getByText("赛事开始后展示成绩。")).toBeDefined();
	});
});
