// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import type { ComponentType } from "react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import type {
	PlatformNetworkAllocation,
	PlatformNetworkHealth,
	PlatformNetworkSettings,
} from "@/api/awd";

const fixtures = vi.hoisted(() => {
	const settings: PlatformNetworkSettings = {
		gamebox_pool: "10.10.0.0/16",
		gamebox_event_prefix: 20,
		gamebox_team_prefix: 24,
		wireguard_pool: "10.20.0.0/16",
		wireguard_event_prefix: 20,
		wireguard_team_prefix: 24,
		wireguard_port_min: 51820,
		wireguard_port_max: 51830,
		wireguard_public_endpoint: "vpn.example.com:51820",
		updated_at: "2026-01-01T00:00:00+00:00",
		gamebox_event_capacity: 16,
		gamebox_team_capacity_per_event: 15,
		gamebox_hosts_per_team: 256,
		wireguard_event_capacity: 16,
		wireguard_team_capacity_per_event: 15,
		wireguard_port_capacity: 11,
	};
	const health: PlatformNetworkHealth = {
		nftables: "Healthy (v1.0.9)",
		wireguard: "Missing",
		docker: "Available",
		firewall_runtime: "native nftables",
		floatctf_table: "inet floatctf_awd",
		docker_firewall_backend: "iptables",
		firewalld: "active",
		ipv4_forwarding: "enabled",
		ipv6_policy: "blocked",
		capability_supported: true,
		notes: ["需要保持 nftables 规则未被外部工具覆盖。"],
	};
	const allocations: PlatformNetworkAllocation[] = [
		{
			event_id: "11111111-1111-1111-1111-111111111111",
			event_title: "示例赛事",
			kind: "gamebox",
			cidr: "10.10.0.0/20",
			allocated_at: "2026-01-01T00:00:00+00:00",
			released_at: null,
			active: true,
		},
		{
			event_id: "22222222-2222-2222-2222-222222222222",
			event_title: null,
			kind: "wireguard",
			cidr: "10.20.0.0/20",
			allocated_at: "2026-01-01T00:00:00+00:00",
			released_at: "2026-01-02T00:00:00+00:00",
			active: false,
		},
	];
	return {
		settings,
		health,
		allocations,
		updatePlatformNetwork: vi.fn(async () => ({ data: {} })),
	};
});

// @/api/axios 会反向导入 @/main（路由实例），单测中必须隔离，否则会加载整棵路由树。
vi.mock("@/api/axios", () => ({ admin_api: {}, service_api: {} }));

vi.mock("@/api", () => ({
	serviceApi: { downloadFile: vi.fn() },
	adminApi: {
		awd: {
			getPlatformNetwork: vi.fn(async () => ({ data: fixtures.settings })),
			getPlatformNetworkHealth: vi.fn(async () => ({ data: fixtures.health })),
			getPlatformNetworkAllocations: vi.fn(async () => ({
				data: fixtures.allocations,
			})),
			updatePlatformNetwork: fixtures.updatePlatformNetwork,
		},
	},
}));

import { Route } from "@/routes/admin/awd/network";

const Page = (Route as unknown as { options: { component: ComponentType } })
	.options.component;

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
	class ResizeObserverStub {
		observe() {}
		unobserve() {}
		disconnect() {}
	}
	globalThis.ResizeObserver = globalThis.ResizeObserver ?? ResizeObserverStub;
});

afterEach(() => {
	cleanup();
	fixtures.updatePlatformNetwork.mockClear();
});

function renderPage() {
	const client = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	});
	return render(
		<QueryClientProvider client={client}>
			<Page />
		</QueryClientProvider>,
	);
}

describe("平台网络配置页", () => {
	it("以中文名称展示配置项，并保留原始配置键", async () => {
		renderPage();

		expect(await screen.findByText("gamebox_pool")).toBeDefined();
		expect(screen.getByText("wireguard_public_endpoint")).toBeDefined();
		expect(screen.getAllByText("地址池").length).toBeGreaterThanOrEqual(3);
		expect(screen.getByText(/地址池划分方式/)).toBeDefined();
		// 表单值来自 GET 返回值
		expect(screen.getByPlaceholderText("10.10.0.0/16")).toHaveProperty(
			"value",
			"10.10.0.0/16",
		);
		expect(screen.getByPlaceholderText("vpn.example.com:51820")).toHaveProperty(
			"value",
			"vpn.example.com:51820",
		);
	});

	it("展示后端返回的容量数值、分配账本与宿主状态", async () => {
		renderPage();

		expect(await screen.findByText("GameBox 可容纳赛事数")).toBeDefined();
		expect(screen.getAllByText("16").length).toBeGreaterThanOrEqual(2);
		expect(screen.getByText("WireGuard 可用端口数")).toBeDefined();
		expect(screen.getByText("11")).toBeDefined();
		// 宿主状态原始值 → 可读状态
		expect(screen.getAllByText("正常").length).toBeGreaterThanOrEqual(1);
		expect(screen.getAllByText("缺失").length).toBeGreaterThanOrEqual(1);
		expect(screen.getByText("运行中")).toBeDefined();
		expect(screen.getByText("已阻断")).toBeDefined();
		expect(
			screen.getByText("需要保持 nftables 规则未被外部工具覆盖。"),
		).toBeDefined();
		// 分配账本
		expect(screen.getByText("示例赛事")).toBeDefined();
		expect(screen.getAllByText("GameBox 网段").length).toBeGreaterThanOrEqual(
			2,
		);
		expect(screen.getByText("使用中")).toBeDefined();
		expect(screen.getByText("已释放")).toBeDefined();
	});

	it("校验不通过时给出提示并禁用保存", async () => {
		renderPage();

		const eventPrefix = (await screen.findAllByLabelText("赛事子网长度"))[0];
		fireEvent.change(eventPrefix, { target: { value: "12" } });

		expect(
			(await screen.findAllByText(/不得小于地址池长度 \/16。/)).length,
		).toBeGreaterThanOrEqual(1);
		expect(
			screen.getByRole("button", { name: "保存配置" }).hasAttribute("disabled"),
		).toBe(true);
	});

	it("地址池重叠时同时标记两个字段", async () => {
		renderPage();

		const wireguardPool = await screen.findByPlaceholderText("10.20.0.0/16");
		fireEvent.change(wireguardPool, { target: { value: "10.10.128.0/17" } });

		expect(
			(await screen.findAllByText(/两个地址池不允许重叠/)).length,
		).toBeGreaterThanOrEqual(2);
	});

	it("未修改时保存不可用，修改后可一次提交全部配置项", async () => {
		renderPage();

		const pool = await screen.findByPlaceholderText("10.10.0.0/16");
		const save = screen.getByRole("button", { name: "保存配置" });
		expect(save.hasAttribute("disabled")).toBe(true);

		fireEvent.change(pool, { target: { value: "10.99.0.0/16" } });
		await waitFor(() => expect(save.hasAttribute("disabled")).toBe(false));
		fireEvent.click(save);

		await waitFor(() =>
			expect(fixtures.updatePlatformNetwork).toHaveBeenCalledTimes(1),
		);
		expect(fixtures.updatePlatformNetwork).toHaveBeenCalledWith({
			gamebox_pool: "10.99.0.0/16",
			gamebox_event_prefix: 20,
			gamebox_team_prefix: 24,
			wireguard_pool: "10.20.0.0/16",
			wireguard_event_prefix: 20,
			wireguard_team_prefix: 24,
			wireguard_public_endpoint: "vpn.example.com:51820",
			wireguard_port_min: 51820,
			wireguard_port_max: 51830,
		});
	});

	it("放弃修改恢复为已保存配置", async () => {
		renderPage();

		const pool = await screen.findByPlaceholderText("10.10.0.0/16");
		fireEvent.change(pool, { target: { value: "10.99.0.0/16" } });
		expect(pool).toHaveProperty("value", "10.99.0.0/16");

		fireEvent.click(screen.getByRole("button", { name: "放弃修改" }));
		await waitFor(() =>
			expect(screen.getByPlaceholderText("10.10.0.0/16")).toHaveProperty(
				"value",
				"10.10.0.0/16",
			),
		);
		expect(
			screen.getByRole("button", { name: "保存配置" }).hasAttribute("disabled"),
		).toBe(true);
	});
});
