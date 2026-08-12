// @vitest-environment jsdom
/**
 * AwdpPhaseOverview 测试（§32）：
 *  - 格式化：MM:SS / H:MM:SS、时长、千分位
 *  - timeline 计算：四态、break:fix 宽度按真实 duration 比例、
 *    Turn 分隔线（6 / 12+ turns）、marker 位置
 *  - 组件渲染：BREAK / FIX / ENDED 关键文案与 ARIA
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

// Primer SegmentedControl 内部 useMedia（useResponsiveValue）依赖 window.matchMedia，
// jsdom 未实现 → 测试前注入 stub。
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
});

import {
	AwdpPhaseOverview,
	computeTimelineState,
	formatCountdown,
	formatDuration,
	formatEventRemaining,
	formatScore,
} from "../AwdpPhaseOverview";

afterEach(cleanup);

describe("formatCountdown", () => {
	it("MM:SS below 1h", () => {
		expect(formatCountdown(2542)).toBe("42:22");
		expect(formatCountdown(59.5)).toBe("01:00"); // ceil
		expect(formatCountdown(0)).toBe("00:00");
		expect(formatCountdown(-5)).toBe("00:00");
	});
	it("H:MM:SS at/over 1h", () => {
		expect(formatCountdown(3661)).toBe("1:01:01");
		expect(formatCountdown(7200)).toBe("2:00:00");
		expect(formatCountdown(3599)).toBe("59:59");
	});
	it("null/invalid → -", () => {
		expect(formatCountdown(null)).toBe("-");
		expect(formatCountdown(undefined)).toBe("-");
		expect(formatCountdown(Number.NaN)).toBe("-");
	});
});

describe("formatDuration", () => {
	it("minutes / hours", () => {
		expect(formatDuration(1800)).toBe("30m");
		expect(formatDuration(3600)).toBe("1h");
		expect(formatDuration(5400)).toBe("1h 30m");
		expect(formatDuration(0)).toBe("0m");
	});
});

describe("formatEventRemaining", () => {
	it("event 赛事同款 d/h/m/s 分段", () => {
		expect(formatEventRemaining(900)).toBe("15m");
		expect(formatEventRemaining(3599)).toBe("59m 59s");
		expect(formatEventRemaining(3661)).toBe("1h 1m 1s");
		expect(formatEventRemaining(5400)).toBe("1h 30m");
		expect(formatEventRemaining(45)).toBe("45s");
		expect(formatEventRemaining(86400 + 3600)).toBe("1d 1h");
		expect(formatEventRemaining(0)).toBe("0s");
	});
	it("null/invalid → -", () => {
		expect(formatEventRemaining(null)).toBe("-");
		expect(formatEventRemaining(undefined)).toBe("-");
		expect(formatEventRemaining(Number.NaN)).toBe("-");
	});
});

describe("formatScore", () => {
	it("thousands separator", () => {
		expect(formatScore(1000)).toBe("1,000");
		expect(formatScore(1234567)).toBe("1,234,567");
		expect(formatScore(0)).toBe("0");
	});
});

describe("computeTimelineState", () => {
	const now = Date.parse("2026-08-12T08:00:00Z");

	it("BREAK：30m break / 90m fix → 25/75 宽度，marker 按 elapsed", () => {
		const s = computeTimelineState({
			phase: "break",
			breakDurationSecs: 1800,
			fixDurationSecs: 5400,
			totalRounds: 6,
			startedAt: new Date(now - 900_000).toISOString(), // 15min elapsed
			now,
		});
		expect(s.breakWidthPct).toBeCloseTo(25);
		expect(s.fixWidthPct).toBeCloseTo(75);
		expect(s.progress).toBeCloseTo(900 / 7200, 5);
		expect(s.markerPct).toBeCloseTo((900 / 7200) * 100, 5);
		expect(s.breakFillPct).toBeCloseTo(50);
		expect(s.fixFillPct).toBe(0);
		expect(s.elapsedBreakSecs).toBeCloseTo(900, 5);
	});

	it("BREAK：elapsed 超过 break 时长时 fill 封顶 100", () => {
		const s = computeTimelineState({
			phase: "break",
			breakDurationSecs: 1800,
			fixDurationSecs: 5400,
			totalRounds: 6,
			startedAt: new Date(now - 3600_000).toISOString(),
			now,
		});
		expect(s.breakFillPct).toBe(100);
	});

	it("FIX：break 完成 + fix 进度，marker 越过 break/fix 边界", () => {
		const s = computeTimelineState({
			phase: "fix",
			breakDurationSecs: 3600,
			fixDurationSecs: 5400,
			totalRounds: 6,
			startedAt: new Date(now - 3600_000).toISOString(),
			fixStartedAt: new Date(now - 300_000).toISOString(), // 5min into fix
			now,
		});
		expect(s.breakFillPct).toBe(100);
		expect(s.fixFillPct).toBeCloseTo((300 / 5400) * 100, 5);
		expect(s.progress).toBeCloseTo((3600 + 300) / 9000, 5);
		expect(s.markerPct).toBeGreaterThan(40); // 越过 3600/9000=40% 边界
	});

	it("ENDED：progress 1 / 全满 / marker 100", () => {
		const s = computeTimelineState({
			phase: "ended",
			breakDurationSecs: 3600,
			fixDurationSecs: 5400,
			totalRounds: 6,
			startedAt: new Date(now - 9000_000).toISOString(),
			fixStartedAt: new Date(now - 5400_000).toISOString(),
			now,
		});
		expect(s.progress).toBe(1);
		expect(s.breakFillPct).toBe(100);
		expect(s.fixFillPct).toBe(100);
		expect(s.markerPct).toBe(100);
	});

	it("PENDING：zero progress / 无 fill", () => {
		const s = computeTimelineState({
			phase: "pending",
			breakDurationSecs: 3600,
			fixDurationSecs: 5400,
			totalRounds: 6,
			now,
		});
		expect(s.progress).toBe(0);
		expect(s.markerPct).toBe(0);
		expect(s.breakFillPct).toBe(0);
		expect(s.fixFillPct).toBe(0);
	});

	it("6 turns：5 条分隔线等分 Fix 段", () => {
		const s = computeTimelineState({
			phase: "fix",
			breakDurationSecs: 3600,
			fixDurationSecs: 3600, // 50/50
			totalRounds: 6,
			startedAt: new Date(now - 3600_000).toISOString(),
			fixStartedAt: new Date(now).toISOString(),
			now,
		});
		expect(s.turnBoundariesPct).toHaveLength(5);
		expect(s.turnBoundariesPct[0]).toBeCloseTo(50 + 50 / 6);
		expect(s.turnBoundariesPct[4]).toBeCloseTo(50 + 250 / 6);
	});

	it("12+ turns：分隔线数量正确且不越界", () => {
		const s = computeTimelineState({
			phase: "fix",
			breakDurationSecs: 3600,
			fixDurationSecs: 7200,
			totalRounds: 30,
			startedAt: new Date(now - 3600_000).toISOString(),
			fixStartedAt: new Date(now).toISOString(),
			now,
		});
		expect(s.turnBoundariesPct).toHaveLength(29);
		for (const p of s.turnBoundariesPct) {
			expect(p).toBeGreaterThan(s.breakWidthPct);
			expect(p).toBeLessThan(100);
		}
	});
});

describe("AwdpPhaseOverview 渲染", () => {
	const now = Date.parse("2026-08-12T08:00:00Z");
	const base = {
		startedAt: new Date(now - 900_000).toISOString(),
		breakEndsAt: new Date(now + 900_000).toISOString(),
		fixStartedAt: null,
		fixEndsAt: null,
		breakDurationSecs: 1800,
		fixDurationSecs: 5400,
		currentRound: 0,
		totalRounds: 6,
		nextCheckAt: null,
		score: 1000,
		now,
	};

	it("BREAK（竞赛）：badge / 描述 / 事件式倒计时 / Score / timeline ARIA", () => {
		render(<AwdpPhaseOverview phase="break" {...base} />);
		expect(screen.getByText("BREAK")).toBeDefined();
		expect(
			screen.getByText("Exploit the target and submit the flag"),
		).toBeDefined();
		// event 赛事同款倒计时文案：剩余 900s
		expect(screen.getByText("Ends in: 15m")).toBeDefined();
		expect(screen.getByText("Score")).toBeDefined();
		expect(screen.getByText("1,000")).toBeDefined();
		// Break 阶段不显示 Turn 文案
		expect(screen.queryByText(/Turn \d/)).toBeNull();
		const bar = screen.getByRole("progressbar");
		expect(bar.getAttribute("aria-valuenow")).toBe("13"); // 900/7200≈12.5→13
		expect(bar.getAttribute("aria-valuemin")).toBe("0");
		expect(bar.getAttribute("aria-valuemax")).toBe("100");
		// 竞赛模式无阶段控制/End
		expect(screen.queryByRole("button", { name: /Fix/i })).toBeNull();
		expect(screen.queryByRole("button", { name: /End/i })).toBeNull();
	});

	it("FIX（竞赛）：badge / Turn / next check / 事件式倒计时", () => {
		render(
			<AwdpPhaseOverview
				phase="fix"
				{...base}
				currentRound={3}
				startedAt={new Date(now - 3600_000).toISOString()}
				fixStartedAt={new Date(now - 300_000).toISOString()}
				fixEndsAt={new Date(now + 5100_000).toISOString()}
				nextCheckAt={new Date(now + 300_000).toISOString()}
			/>,
		);
		expect(screen.getByText("FIX")).toBeDefined();
		expect(
			screen.getByText("Patch the service before the next evaluation"),
		).toBeDefined();
		expect(screen.getAllByText("Turn 3 / 6").length).toBeGreaterThanOrEqual(1);
		expect(screen.getByText("Next check in: 5m")).toBeDefined(); // 300s
		expect(screen.getByText("Score")).toBeDefined();
	});

	it("ENDED：Final Score / Finished / 无 next check", () => {
		render(<AwdpPhaseOverview phase="ended" {...base} score={1750} />);
		expect(screen.getByText("ENDED")).toBeDefined();
		expect(screen.getByText("Final Score")).toBeDefined();
		expect(screen.getByText("1,750")).toBeDefined();
		expect(screen.getByText("Finished")).toBeDefined();
		expect(screen.queryByText(/Next check in/)).toBeNull();
		expect(
			screen.getByRole("progressbar").getAttribute("aria-valuenow"),
		).toBe("100");
	});

	it("PENDING：Waiting to start / zero progress", () => {
		render(
			<AwdpPhaseOverview
				phase="pending"
				{...base}
				startedAt={null}
				nextCheckAt={null}
			/>,
		);
		expect(screen.getByText("NOT STARTED")).toBeDefined();
		expect(screen.getByText("Waiting to start")).toBeDefined();
		expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe(
			"0",
		);
	});

	it("练习模式：SegmentedControl Break|Fix + End（space-between 最右侧）", () => {
		const onSetPhase = vi.fn();
		const onEnd = vi.fn();
		render(
			<AwdpPhaseOverview
				phase="break"
				{...base}
				canControlPhase
				onSetPhase={onSetPhase}
				onEnd={onEnd}
			/>,
		);
		// 阶段控制按钮
		expect(screen.getByRole("button", { name: /^Break$/ })).toBeDefined();
		expect(screen.getByRole("button", { name: /^Fix$/ })).toBeDefined();
		// End 在最右侧（space-between 最右侧元素）
		const endBtn = screen.getByRole("button", { name: /^End$/ });
		expect(endBtn).toBeDefined();
		// 点击 Fix 切换阶段
		fireEvent.click(screen.getByRole("button", { name: /^Fix$/ }));
		expect(onSetPhase).toHaveBeenCalledWith("fix");
		// 点击 End
		fireEvent.click(endBtn);
		expect(onEnd).toHaveBeenCalledTimes(1);
	});

	it("练习模式：当前阶段在 SegmentedControl 中 selected；phaseBusy 禁用按钮", () => {
		const onSetPhase = vi.fn();
		render(
			<AwdpPhaseOverview
				phase="fix"
				{...base}
				currentRound={2}
				canControlPhase
				onSetPhase={onSetPhase}
				phaseBusy
			/>,
		);
		const fixBtn = screen.getByRole("button", { name: /^Fix$/ });
		expect(fixBtn.getAttribute("aria-current")).toBe("true");
		expect(fixBtn.hasAttribute("disabled")).toBe(true);
		expect(
			screen.getByRole("button", { name: /^Break$/ }).hasAttribute("disabled"),
		).toBe(true);
		// Fix 阶段显示 Turn
		expect(screen.getAllByText("Turn 2 / 6").length).toBeGreaterThanOrEqual(1);
	});

	it("练习模式：点击当前阶段不改（same-phase no-op）", () => {
		const onSetPhase = vi.fn();
		render(
			<AwdpPhaseOverview
				phase="break"
				{...base}
				canControlPhase
				onSetPhase={onSetPhase}
			/>,
		);
		fireEvent.click(screen.getByRole("button", { name: /^Break$/ }));
		expect(onSetPhase).not.toHaveBeenCalled();
	});
});
