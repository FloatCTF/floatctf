import { describe, expect, it } from "vitest";

import type { UniResponse } from "../protocol";
import {
	FloatCTFError,
	floatCTFErrorFromEnvelope,
	toFloatCTFError,
} from "../errors";

describe("FloatCTFError", () => {
	it("exposes typed fields and an axios-compatible response shape", () => {
		const error = new FloatCTFError({
			message: "Request failed with status code 403",
			kind: "http",
			httpStatus: 403,
			code: 4030,
			platformMessage: "没有权限执行该操作",
			response: { status: 403, data: { code: 4030, message: "没有权限执行该操作" } },
		});
		expect(error.name).toBe("FloatCTFError");
		expect(error.kind).toBe("http");
		expect(error.httpStatus).toBe(403);
		expect(error.code).toBe(4030);
		expect(error.platformMessage).toBe("没有权限执行该操作");
		expect(error.displayMessage).toBe("没有权限执行该操作");
		expect(error.unauthorized).toBe(false);
		expect(error.response?.status).toBe(403);
	});

	it("marks 401 as unauthorized", () => {
		const error = new FloatCTFError({ message: "401", kind: "http", httpStatus: 401 });
		expect(error.unauthorized).toBe(true);
	});

	it("falls back to message for displayMessage", () => {
		const error = new FloatCTFError({ message: "Network Error", kind: "network" });
		expect(error.displayMessage).toBe("Network Error");
	});
});

describe("toFloatCTFError", () => {
	it("is idempotent", () => {
		const error = new FloatCTFError({ message: "x", kind: "unknown" });
		expect(toFloatCTFError(error)).toBe(error);
	});

	it("normalizes an axios-like HTTP error", () => {
		const axiosError = Object.assign(new Error("Request failed with status code 404"), {
			isAxiosError: true,
			response: { status: 404, statusText: "Not Found", data: { code: 404, message: "不存在" }, headers: {} },
		});
		const normalized = toFloatCTFError(axiosError);
		expect(normalized.httpStatus).toBe(404);
		expect(normalized.platformMessage).toBe("不存在");
		expect(normalized.kind).toBe("http");
		expect(normalized.original).toBe(axiosError);
	});

	it("normalizes a network error without a response", () => {
		const axiosError = Object.assign(new Error("Network Error"), {
			isAxiosError: true,
			code: "ERR_NETWORK",
		});
		const normalized = toFloatCTFError(axiosError);
		expect(normalized.kind).toBe("network");
		expect(normalized.httpStatus).toBeUndefined();
		expect(normalized.response).toBeUndefined();
	});

	it("handles a plain Error and a non-Error value", () => {
		expect(toFloatCTFError(new Error("plain")).kind).toBe("unknown");
		expect(toFloatCTFError("string failure").message).toBe("string failure");
	});
});

describe("floatCTFErrorFromEnvelope", () => {
	it("returns null for a successful envelope", () => {
		const ok: UniResponse<{ id: string }> = { code: 0, message: "ok", data: { id: "x" } };
		expect(floatCTFErrorFromEnvelope(ok)).toBeNull();
	});

	it("returns a platform error for a non-zero code", () => {
		const error = floatCTFErrorFromEnvelope({ code: 7, message: "业务失败" });
		expect(error?.kind).toBe("platform");
		expect(error?.code).toBe(7);
		expect(error?.displayMessage).toBe("业务失败");
	});

	it("ignores non-envelope payloads", () => {
		expect(floatCTFErrorFromEnvelope(null)).toBeNull();
		expect(floatCTFErrorFromEnvelope("text")).toBeNull();
	});
});
