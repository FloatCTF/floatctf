/**
 * AWD 失效列表测试（风险清单 #11 的回归护栏）。
 *
 * SSE 连上后轮询回退会停止，所以事件流的失效列表必须覆盖页面实际读取的全部 query key，
 * 否则面板会「看起来正常但从不更新」。这里把两份列表钉住：新增 AWD 页面 key 时，
 * 忘了同步 awdInvalidation.ts 就会红。
 */
import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";

import {
	AWD_ADMIN_QUERY_KEYS,
	AWD_PLAYER_QUERY_KEYS,
	invalidateAwdQueries,
} from "../awdInvalidation";

describe("AWD 失效列表（SSE 与页面手动刷新共用）", () => {
	it("选手端覆盖页面与 hook 实际读取的 key", () => {
		expect([...AWD_PLAYER_QUERY_KEYS]).toEqual(
			expect.arrayContaining([
				"eventInfo",
				"event",
				"awd-gameboxes",
				"awd-scores",
				"awd-wg",
				"awd-ssh",
				"awd-player-status",
				"announcements",
			]),
		);
	});

	it("管理端覆盖三个管理页在读的 admin-awd-status 与 prechecks", () => {
		expect([...AWD_ADMIN_QUERY_KEYS]).toEqual(
			expect.arrayContaining([
				"event",
				"eventInfo",
				"awd-gameboxes",
				"awd-scores",
				"admin-awd-scores",
				"admin-awd-status",
				"admin-awd-prechecks",
			]),
		);
	});

	it("按 [key, eventId] 逐条失效", () => {
		const qc = new QueryClient();
		const spy = vi.spyOn(qc, "invalidateQueries");

		invalidateAwdQueries(qc, "evt-1", AWD_PLAYER_QUERY_KEYS);

		expect(spy).toHaveBeenCalledTimes(AWD_PLAYER_QUERY_KEYS.length);
		for (const key of AWD_PLAYER_QUERY_KEYS) {
			expect(spy).toHaveBeenCalledWith({ queryKey: [key, "evt-1"] });
		}
	});
});
