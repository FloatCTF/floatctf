// query options 工厂必须保持与原先 useQuery 完全相同的 query key，
// 因为既有代码按 key 做 invalidate/refetch（例如 useAwdEventStream.invalidateQueries）。
import type { FloatCTFClient } from "@floatctf/sdk";
import { describe, expect, it } from "vitest";

import { createQueryFactories } from "../queries/index";

/** 只用 key 的工厂不需要真的发请求：给一个最小可用的假客户端即可。 */
function fakeClient(): FloatCTFClient {
	return {
		service: {
			events: { get: async () => ({ code: 0, message: "ok" }) },
			challenges: {
				get: async () => ({ code: 0, message: "ok" }),
				getInstance: async () => ({ code: 0, message: "ok" }),
			},
		},
		admin: { system: { monitor: async () => ({ code: 0, message: "ok" }) } },
	} as unknown as FloatCTFClient;
}

const factories = createQueryFactories(fakeClient());

describe("query options factory keys", () => {
	it("eventInfo keeps key ['eventInfo', id]", () => {
		expect(factories.eventInfoQueryOptions("abc-123").queryKey).toEqual([
			"eventInfo",
			"abc-123",
		]);
	});

	it("challenge keeps key ['challenge', id]", () => {
		expect(factories.challengeQueryOptions("abc-123").queryKey).toEqual([
			"challenge",
			"abc-123",
		]);
	});

	it("instance keeps key ['instance', id]", () => {
		expect(factories.challengeInstanceQueryOptions("abc-123").queryKey).toEqual([
			"instance",
			"abc-123",
		]);
	});

	it("system_information keeps key ['system_information']", () => {
		expect(factories.systemInformationQueryOptions().queryKey).toEqual([
			"system_information",
		]);
	});
});
