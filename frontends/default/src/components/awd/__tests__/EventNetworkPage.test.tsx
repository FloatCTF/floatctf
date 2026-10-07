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
import {
	afterEach,
	beforeAll,
	beforeEach,
	describe,
	expect,
	it,
	vi,
} from "vitest";

import type { EventNetworkInfo } from "@floatctf/sdk";

const fixtures = vi.hoisted(() => {
	const EVENT_ID = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
	const network: EventNetworkInfo = {
		event_id: EVENT_ID,
		allocation_mode: "automatic",
		gamebox_cidr: "10.96.0.0/16",
		wireguard_cidr: "10.112.0.0/16",
		infrastructure_subnet: "10.96.0.0/28",
		flagserver_ip: "10.96.0.2",
		judgeserver_ip: "10.96.0.3",
		wireguard_interface_name: "wg-awd-aaaa",
		wireguard_listen_port: 51820,
		docker_network_name: "awd-aaaa",
		locked: false,
	};
	return {
		EVENT_ID,
		network,
		getEventNetwork: vi.fn(),
		allocateEventNetwork: vi.fn(async () => ({ data: {} })),
		reallocateEventNetwork: vi.fn(async () => ({ data: {} })),
	};
});

vi.mock("@/api", () => ({
	serviceApi: { downloadFile: vi.fn() },
	adminApi: {
		awd: {
			getEventNetwork: fixtures.getEventNetwork,
			allocateEventNetwork: fixtures.allocateEventNetwork,
			reallocateEventNetwork: fixtures.reallocateEventNetwork,
		},
	},
}));

// 页面通过 Route.useParams() 取赛事 id；单测里用固定的路由参数替代 RouterProvider。
vi.mock("@tanstack/react-router", async (importOriginal) => {
	const actual =
		await importOriginal<typeof import("@tanstack/react-router")>();
	return {
		...actual,
		createFileRoute: () => (options: unknown) => ({
			options,
			useParams: () => ({ id: fixtures.EVENT_ID }),
		}),
	};
});

import { Route } from "@/routes/admin/events/awd.$id/network";

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
	// Primer Dialog（useConfirm）内部 useOverflow 依赖 ResizeObserver（jsdom 缺失）。
	class ResizeObserverStub {
		observe() {}
		unobserve() {}
		disconnect() {}
	}
	globalThis.ResizeObserver = globalThis.ResizeObserver ?? ResizeObserverStub;
});

beforeEach(() => {
	fixtures.getEventNetwork.mockReset();
	fixtures.allocateEventNetwork.mockClear();
	fixtures.reallocateEventNetwork.mockClear();
});

afterEach(() => {
	cleanup();
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

describe("赛事网络页", () => {
	it("未分配时提供自动分配，点击后按平台地址池分配", async () => {
		fixtures.getEventNetwork.mockResolvedValue({ data: null });
		renderPage();

		expect(await screen.findByText("未分配")).toBeDefined();
		expect(screen.getByText("gamebox_cidr")).toBeDefined();
		expect(screen.getByText("wireguard_cidr")).toBeDefined();

		fireEvent.click(screen.getByRole("button", { name: "自动分配网络" }));
		await waitFor(() =>
			expect(fixtures.allocateEventNetwork).toHaveBeenCalledWith(
				fixtures.EVENT_ID,
				{},
			),
		);
	});

	it("手动分配：校验非法网段并提交完整请求体", async () => {
		fixtures.getEventNetwork.mockResolvedValue({ data: null });
		renderPage();

		const manualButton = await screen.findByRole("button", {
			name: "按手动配置分配",
		});
		expect(manualButton.hasAttribute("disabled")).toBe(true);

		const gameboxCidr = screen.getByPlaceholderText("10.96.0.0/16");
		fireEvent.change(gameboxCidr, { target: { value: "10.96.0.0" } });
		expect(await screen.findByText(/格式不正确，应为 CIDR/)).toBeDefined();
		expect(manualButton.hasAttribute("disabled")).toBe(true);

		// /20 不满足 AWD 运行时要求（赛事网段须 /16 或更大）
		fireEvent.change(gameboxCidr, { target: { value: "10.96.0.0/20" } });
		expect(await screen.findByText(/不得小于 \/16/)).toBeDefined();
		expect(manualButton.hasAttribute("disabled")).toBe(true);

		fireEvent.change(gameboxCidr, { target: { value: "10.96.0.0/16" } });
		fireEvent.change(screen.getByPlaceholderText("10.112.0.0/16"), {
			target: { value: "10.112.0.0/16" },
		});
		fireEvent.change(screen.getByPlaceholderText("51820"), {
			target: { value: "51820" },
		});

		await waitFor(() =>
			expect(manualButton.hasAttribute("disabled")).toBe(false),
		);
		fireEvent.click(manualButton);

		await waitFor(() =>
			expect(fixtures.allocateEventNetwork).toHaveBeenCalledWith(
				fixtures.EVENT_ID,
				{
					allocation_mode: "manual",
					gamebox_cidr: "10.96.0.0/16",
					wireguard_cidr: "10.112.0.0/16",
					wireguard_listen_port: 51820,
				},
			),
		);
	});

	it("已分配时展示中文网络详情与真实取值", async () => {
		fixtures.getEventNetwork.mockResolvedValue({ data: fixtures.network });
		renderPage();

		expect(await screen.findByText("已分配")).toBeDefined();
		expect(screen.getByText("自动分配")).toBeDefined();
		expect(screen.getByText("10.96.0.0/16")).toBeDefined();
		expect(screen.getByText("10.112.0.0/16")).toBeDefined();
		expect(screen.getByText("10.96.0.0/28")).toBeDefined();
		expect(screen.getByText("10.96.0.2")).toBeDefined();
		expect(screen.getByText("10.96.0.3")).toBeDefined();
		expect(screen.getByText("wg-awd-aaaa")).toBeDefined();
		expect(screen.getByText("awd-aaaa")).toBeDefined();
		expect(screen.getByText("infrastructure_subnet")).toBeDefined();
		expect(screen.getByRole("button", { name: "重新分配网络" })).toBeDefined();
	});

	it("重新分配需要确认后才会调用接口", async () => {
		fixtures.getEventNetwork.mockResolvedValue({ data: fixtures.network });
		renderPage();

		fireEvent.click(
			await screen.findByRole("button", { name: "重新分配网络" }),
		);
		expect(await screen.findByText("确认重新分配网络？")).toBeDefined();
		expect(fixtures.reallocateEventNetwork).not.toHaveBeenCalled();

		fireEvent.click(screen.getByRole("button", { name: "OK" }));
		await waitFor(() =>
			expect(fixtures.reallocateEventNetwork).toHaveBeenCalledWith(
				fixtures.EVENT_ID,
			),
		);
	});

	it("已锁定时给出提示且不提供重新分配", async () => {
		fixtures.getEventNetwork.mockResolvedValue({
			data: { ...fixtures.network, locked: true, allocation_mode: "manual" },
		});
		renderPage();

		expect(await screen.findByText("已锁定")).toBeDefined();
		expect(screen.getByText("手动指定")).toBeDefined();
		expect(
			screen.getByText(/赛事已部署，网络地址已锁定，不能重新分配。/),
		).toBeDefined();
		expect(screen.queryByRole("button", { name: "重新分配网络" })).toBeNull();
	});
});
