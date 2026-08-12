// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import type { AwdpPhase } from "@/api/awdp";
import {
	AwdpWorkbench,
	type AwdpWorkbenchGameBox,
	type AwdpWorkbenchViewModel,
} from "../AwdpWorkbench";

// Primer SegmentedControl 内部 useMedia 依赖 window.matchMedia（jsdom 缺失）。
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
	globalThis.ResizeObserver =
		globalThis.ResizeObserver ?? ResizeObserverStub;
});

afterEach(() => {
	cleanup();
});

const gb: AwdpWorkbenchGameBox = {
	id: "gb1",
	gamebox_id: "gb1",
	name: "test_g",
	category: "web",
	broken: false,
	enabled: true,
	source_code_dir: "/var/www/html",
	instance: {
		instance_id: "i1",
		runtime_state: "running",
		runtime_generation: 1,
		endpoints: [],
	},
};

const vm = (
	phase: AwdpPhase,
	overrides?: Partial<AwdpWorkbenchViewModel>,
): AwdpWorkbenchViewModel => {
	const fixAt = new Date(Date.now() + 600_000).toISOString();
	return {
		title: "test_g",
		description: "hello floatctf",
		phase,
		startedAt: new Date().toISOString(),
		breakEndsAt: new Date(Date.now() + 3_600_000).toISOString(),
		fixStartedAt: phase === "fix" ? new Date().toISOString() : null,
		fixEndsAt: phase === "fix" ? fixAt : null,
		breakDurationSecs: 3600,
		fixDurationSecs: 3600,
		phaseEndsAt: phase === "fix" ? fixAt : new Date(Date.now() + 3_600_000).toISOString(),
		currentRound: phase === "fix" ? 1 : 0,
		totalRounds: 6,
		nextCheckAt: phase === "fix" ? fixAt : null,
		score: 0,
		breakScore: 1000,
		fixRoundScore: 150,
		gameboxes: [gb],
		history: [],
		isPractice: true,
		canControlPhase: true,
		...overrides,
	};
};

const noopProps = {
	onSubmitBreak: vi.fn(),
	onUploadPatch: vi.fn(),
	onTestCheck: vi.fn(),
};

describe("AwdpWorkbench 按钮互斥禁用", () => {
	it("Reset 进行中：本卡片所有按钮禁用（练习 Break：Reset + Submit），阶段控制/End 保持可用", async () => {
		let resolveReset: (() => void) | undefined;
		const onResetInstance = vi.fn(
			(_id: string) =>
				new Promise<void>((res) => {
					resolveReset = res;
				}),
		);
		render(
			<AwdpWorkbench
				viewModel={vm("break")}
				{...noopProps}
				onResetInstance={onResetInstance}
				onSetPhase={vi.fn()}
				onEnd={vi.fn()}
			/>,
		);

		// Submit 先置为可用（输入 flag）
		fireEvent.change(screen.getByPlaceholderText("flag{...}"), {
			target: { value: "flag{1}" },
		});
		const submit = screen.getByRole("button", { name: /^Submit$/ });
		const resetBtn = screen.getByRole("button", { name: /^Reset$/ });
		expect(resetBtn.hasAttribute("disabled")).toBe(false);
		expect(submit.hasAttribute("disabled")).toBe(false);

		fireEvent.click(resetBtn);

		// pending：Reset 显示 Resetting… 且禁用，Submit 一并禁用
		expect(screen.getByRole("button", { name: /Resetting/ })).toBeDefined();
		expect(submit.hasAttribute("disabled")).toBe(true);
		// reset 是卡片级：阶段控制与 End 不受影响
		expect(
			screen.getByRole("button", { name: /^Break$/ }).hasAttribute("disabled"),
		).toBe(false);
		expect(
			screen.getByRole("button", { name: /^End$/ }).hasAttribute("disabled"),
		).toBe(false);

		resolveReset?.();
		await waitFor(() =>
			expect(screen.getByRole("button", { name: /^Reset$/ })).toBeDefined(),
		);
		expect(submit.hasAttribute("disabled")).toBe(false);
	});

	it("Reset 进行中：Fix 阶段全部 fix 按钮禁用（Source/Patch/Test Check/提前 Check）", async () => {
		let resolveReset: (() => void) | undefined;
		const onResetInstance = vi.fn(
			(_id: string) =>
				new Promise<void>((res) => {
					resolveReset = res;
				}),
		);
		render(
			<AwdpWorkbench
				viewModel={vm("fix")}
				{...noopProps}
				onResetInstance={onResetInstance}
				onDownloadSource={vi.fn(async () => "https://example.com")}
				onEarlyCheck={vi.fn()}
			/>,
		);

		fireEvent.click(screen.getByRole("button", { name: /^Reset$/ }));

		expect(screen.getByRole("button", { name: /Resetting/ })).toBeDefined();
		expect(
			screen
				.getByRole("button", { name: /^Download Source$/ })
				.hasAttribute("disabled"),
		).toBe(true);
		expect(
			screen.getByRole("button", { name: /^Apply Patch$/ }).hasAttribute(
				"disabled",
			),
		).toBe(true);
		expect(
			screen.getByRole("button", { name: /^Test Check$/ }).hasAttribute(
				"disabled",
			),
		).toBe(true);
		expect(
			screen.getByRole("button", { name: /^提前 Check$/ }).hasAttribute(
				"disabled",
			),
		).toBe(true);

		resolveReset?.();
		await waitFor(() =>
			expect(screen.getByRole("button", { name: /^Reset$/ })).toBeDefined(),
		);
	});

	it("阶段切换（SegmentedControl）进行中：SegmentedControl/End/卡片全部按钮禁用", async () => {
		let resolvePhase: (() => void) | undefined;
		const onSetPhase = vi.fn(
			(_t: "break" | "fix") =>
				new Promise<void>((res) => {
					resolvePhase = res;
				}),
		);
		render(
			<AwdpWorkbench
				viewModel={vm("break")}
				{...noopProps}
				onSetPhase={onSetPhase}
				onResetInstance={vi.fn()}
				onEnd={vi.fn()}
			/>,
		);

		// Submit 先置为可用（输入 flag）
		fireEvent.change(screen.getByPlaceholderText("flag{...}"), {
			target: { value: "flag{1}" },
		});
		expect(
			screen.getByRole("button", { name: /^Submit$/ }).hasAttribute("disabled"),
		).toBe(false);

		fireEvent.click(screen.getByRole("button", { name: /^Fix$/ }));
		await waitFor(() => expect(onSetPhase).toHaveBeenCalledWith("fix"));

		// pending：全部禁用（End 仍显示「End」，仅禁用；「停止中…」只在 End 本身进行中）
		const expectedDisabled = [
			/^Break$/,
			/^Fix$/,
			/^End$/,
			/^Reset$/,
			/^Submit$/,
		];
		for (const name of expectedDisabled) {
			expect(
				screen.getByRole("button", { name }).hasAttribute("disabled"),
			).toBe(true);
		}

		resolvePhase?.();
		await waitFor(() =>
			expect(
				screen.getByRole("button", { name: /^Fix$/ }).hasAttribute("disabled"),
			).toBe(false),
		);
		expect(screen.getByRole("button", { name: /^End$/ })).toBeDefined();
	});

	it("End 本身进行中才显示「停止中…」；阶段切换期间 End 保持「End」文案仅禁用", async () => {
		let resolveEnd: (() => void) | undefined;
		let resolvePhase: (() => void) | undefined;
		const onEnd = vi.fn(
			() =>
				new Promise<void>((res) => {
					resolveEnd = res;
				}),
		);
		const onSetPhase = vi.fn(
			(_t: "break" | "fix") =>
				new Promise<void>((res) => {
					resolvePhase = res;
				}),
		);
		render(
			<AwdpWorkbench
				viewModel={vm("break")}
				{...noopProps}
				onSetPhase={onSetPhase}
				onResetInstance={vi.fn()}
				onEnd={onEnd}
			/>,
		);

		// 1) End 本身进行中：文案「停止中…」且禁用；SegmentedControl 不受影响
		fireEvent.click(screen.getByRole("button", { name: /^End$/ }));
		await waitFor(() => expect(onEnd).toHaveBeenCalled());
		expect(screen.getByRole("button", { name: /停止中/ })).toBeDefined();
		expect(
			screen.getByRole("button", { name: /停止中/ }).hasAttribute("disabled"),
		).toBe(true);
		expect(
			screen.getByRole("button", { name: /^Break$/ }).hasAttribute("disabled"),
		).toBe(false);
		resolveEnd?.();
		await waitFor(() =>
			expect(screen.getByRole("button", { name: /^End$/ })).toBeDefined(),
		);

		// 2) 阶段切换期间：End 保持「End」文案，仅禁用
		fireEvent.click(screen.getByRole("button", { name: /^Fix$/ }));
		await waitFor(() => expect(onSetPhase).toHaveBeenCalledWith("fix"));
		expect(screen.getByRole("button", { name: /^End$/ })).toBeDefined();
		expect(
			screen.getByRole("button", { name: /^End$/ }).hasAttribute("disabled"),
		).toBe(true);
		expect(screen.queryByRole("button", { name: /停止中/ })).toBeNull();
		resolvePhase?.();
	});
});
