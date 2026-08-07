import {
	challengeInstanceQueryOptions,
	challengeQueryOptions,
	eventInfoQueryOptions,
	systemInformationQueryOptions,
} from "@/api/queries";
// @vitest-environment jsdom
// Query options factories must keep the EXACT query keys used by the
// original useQuery calls, because existing code invalidates/refetches
// by key (e.g. useAwdEventStream.invalidateQueries).
import { describe, expect, it } from "vitest";

describe("query options factory keys", () => {
	it("eventInfo keeps key ['eventInfo', id]", () => {
		expect(eventInfoQueryOptions("abc-123").queryKey).toEqual([
			"eventInfo",
			"abc-123",
		]);
	});

	it("challenge keeps key ['challenge', id]", () => {
		expect(challengeQueryOptions("abc-123").queryKey).toEqual([
			"challenge",
			"abc-123",
		]);
	});

	it("instance keeps key ['instance', id]", () => {
		expect(challengeInstanceQueryOptions("abc-123").queryKey).toEqual([
			"instance",
			"abc-123",
		]);
	});

	it("system_information keeps key ['system_information']", () => {
		expect(systemInformationQueryOptions().queryKey).toEqual([
			"system_information",
		]);
	});
});
