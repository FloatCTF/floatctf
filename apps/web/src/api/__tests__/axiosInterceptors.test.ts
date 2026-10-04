// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
	navigate: vi.fn(),
	removeAdminToken: vi.fn(),
	removeToken: vi.fn(),
}));

// 隔离 @/main（路由实例）与登录态 store，只测响应拦截器本身。
vi.mock("@/main", () => ({ router: { navigate: mocks.navigate } }));
vi.mock("@/stores/AuthStore", () => ({
	useAuthStore: {
		getState: () => ({
			adminToken: "admin-token",
			token: "user-token",
			removeAdminToken: mocks.removeAdminToken,
			removeToken: mocks.removeToken,
		}),
	},
}));

import { admin_api, service_api } from "@/api/axios";

type RejectedHandler = (error: unknown) => Promise<unknown>;

function rejectedHandler(instance: typeof admin_api): RejectedHandler {
	const handlers = (
		instance.interceptors.response as unknown as {
			handlers: { rejected?: RejectedHandler }[];
		}
	).handlers;
	const handler = handlers.find((h) => h.rejected)?.rejected;
	if (!handler) {
		throw new Error("响应拦截器未注册 rejected handler");
	}
	return handler;
}

describe("axios 响应拦截器：无 response 的网络错误", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		// 拦截器内部会 console.log(error)，测试里静音。
		vi.spyOn(console, "log").mockImplementation(() => {});
	});

	it("admin_api 原样透传网络错误，不清理管理员登录态", async () => {
		const networkError = Object.assign(new Error("Network Error"), {
			code: "ERR_NETWORK",
		});

		await expect(rejectedHandler(admin_api)(networkError)).rejects.toBe(
			networkError,
		);
		expect(mocks.removeAdminToken).not.toHaveBeenCalled();
		expect(mocks.navigate).not.toHaveBeenCalled();
	});

	it("service_api 原样透传超时错误，不清理用户登录态", async () => {
		const timeoutError = Object.assign(
			new Error("timeout of 10000ms exceeded"),
			{
				code: "ECONNABORTED",
			},
		);

		await expect(rejectedHandler(service_api)(timeoutError)).rejects.toBe(
			timeoutError,
		);
		expect(mocks.removeToken).not.toHaveBeenCalled();
		expect(mocks.navigate).not.toHaveBeenCalled();
	});

	it("admin_api 401 仍然清理管理员登录态并回到 /admin", async () => {
		const unauthorized = { response: { status: 401 } };

		await expect(rejectedHandler(admin_api)(unauthorized)).rejects.toBe(
			unauthorized,
		);
		expect(mocks.removeAdminToken).toHaveBeenCalledTimes(1);
		expect(mocks.navigate).toHaveBeenCalledWith({ to: "/admin" });
	});

	it("service_api 401 仍然清理用户登录态并回到 /", async () => {
		const unauthorized = { response: { status: 401 } };

		await expect(rejectedHandler(service_api)(unauthorized)).rejects.toBe(
			unauthorized,
		);
		expect(mocks.removeToken).toHaveBeenCalledTimes(1);
		expect(mocks.navigate).toHaveBeenCalledWith({ to: "/" });
	});

	it("500 等带响应的错误不影响登录态", async () => {
		const serverError = { response: { status: 500 } };

		await expect(rejectedHandler(admin_api)(serverError)).rejects.toBe(
			serverError,
		);
		expect(mocks.removeAdminToken).not.toHaveBeenCalled();
		expect(mocks.navigate).not.toHaveBeenCalled();
	});
});
