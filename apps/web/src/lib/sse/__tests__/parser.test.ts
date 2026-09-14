/**
 * SSE 解析器单元测试。
 *
 * 覆盖：帧边界、字段解析、跨 chunk 拆分、UTF-8、注释、未知字段、retry。
 */
import { describe, expect, it } from "vitest";
import { createSseParser } from "../parser";

function encode(str: string): Uint8Array {
	return new TextEncoder().encode(str);
}

describe("SSE parser", () => {
	it("parses a single data event", () => {
		const parser = createSseParser();
		const events = parser.push(encode("data: hello\n\n"));
		expect(events).toHaveLength(1);
		expect(events[0]).toMatchObject({
			event: "",
			data: "hello",
		});
	});

	it("parses event + data", () => {
		const parser = createSseParser();
		const events = parser.push(
			encode('event: score.changed\ndata: {"points":10}\n\n'),
		);
		expect(events).toHaveLength(1);
		expect(events[0]).toMatchObject({
			event: "score.changed",
			data: '{"points":10}',
		});
	});

	it("parses id + data", () => {
		const parser = createSseParser();
		const events = parser.push(encode("id: 42\ndata: hello\n\n"));
		expect(events).toHaveLength(1);
		expect(events[0]).toMatchObject({
			event: "",
			data: "hello",
			id: "42",
		});
	});

	it("concatenates multiple data lines with newline", () => {
		const parser = createSseParser();
		const events = parser.push(
			encode("data: line1\ndata: line2\ndata: line3\n\n"),
		);
		expect(events).toHaveLength(1);
		expect(events[0].data).toBe("line1\nline2\nline3");
	});

	it("handles CRLF line endings", () => {
		const parser = createSseParser();
		const events = parser.push(new TextEncoder().encode("data: hello\r\n\r\n"));
		expect(events).toHaveLength(1);
		expect(events[0].data).toBe("hello");
	});

	it("handles stream chunk split in middle of data: field", () => {
		const parser = createSseParser();
		// Split "data: hello\n\n" into two chunks
		let events = parser.push(encode("dat"));
		expect(events).toHaveLength(0);

		events = parser.push(encode("a: hello\n\n"));
		expect(events).toHaveLength(1);
		expect(events[0].data).toBe("hello");
	});

	it("handles stream chunk split in middle of UTF-8 character", () => {
		const parser = createSseParser();
		// "data: café\n\n" — é is 0xC3 0xA9 in UTF-8
		const prefix = encode("data: caf");
		// Split before the é bytes
		let events = parser.push(prefix);
		expect(events).toHaveLength(0);

		// Complete the é and add newlines
		events = parser.push(new Uint8Array([0xc3, 0xa9, 0x0a, 0x0a]));
		expect(events).toHaveLength(1);
		expect(events[0].data).toBe("café");
	});

	it("parses two events in one chunk", () => {
		const parser = createSseParser();
		const events = parser.push(encode("data: first\n\ndata: second\n\n"));
		expect(events).toHaveLength(2);
		expect(events[0].data).toBe("first");
		expect(events[1].data).toBe("second");
	});

	it("handles one event across many chunks", () => {
		const parser = createSseParser();
		let events = parser.push(encode("eve"));
		expect(events).toHaveLength(0);
		events = parser.push(encode("nt: upd"));
		expect(events).toHaveLength(0);
		events = parser.push(encode("ate\ndata: ok\n\n"));
		expect(events).toHaveLength(1);
		expect(events[0].event).toBe("update");
		expect(events[0].data).toBe("ok");
	});

	it("ignores comment keepalive lines", () => {
		const parser = createSseParser();
		let events = parser.push(encode(": keepalive\n\n"));
		expect(events).toHaveLength(0);

		// Comment + data
		events = parser.push(encode(": keepalive\ndata: real\n\n"));
		expect(events).toHaveLength(1);
		expect(events[0].data).toBe("real");
	});

	it("ignores unknown field", () => {
		const parser = createSseParser();
		const events = parser.push(encode("custom: value\ndata: hello\n\n"));
		expect(events).toHaveLength(1);
		expect(events[0].data).toBe("hello");
	});

	it("parses retry field", () => {
		const parser = createSseParser();
		const events = parser.push(encode("retry: 5000\ndata: x\n\n"));
		expect(events).toHaveLength(1);
		expect(events[0].retry).toBe(5000);
	});

	it("ignores invalid retry value", () => {
		const parser = createSseParser();
		const events = parser.push(encode("retry: abc\ndata: x\n\n"));
		expect(events).toHaveLength(1);
		expect(events[0].retry).toBeUndefined();
	});

	it("retains incomplete final frame across push calls", () => {
		const parser = createSseParser();
		// First chunk: complete event + incomplete prefix
		let events = parser.push(encode("data: done\n\ndata: part"));
		expect(events).toHaveLength(1);
		expect(events[0].data).toBe("done");

		// Second chunk: completes the prefix
		events = parser.push(encode("ial\n\n"));
		expect(events).toHaveLength(1);
		expect(events[0].data).toBe("partial");
	});

	it("reset clears internal buffer", () => {
		const parser = createSseParser();
		parser.push(encode("data: part"));
		parser.reset();
		const events = parser.push(encode("data: fresh\n\n"));
		expect(events).toHaveLength(1);
		expect(events[0].data).toBe("fresh");
	});

	it("handles data field with leading space", () => {
		const parser = createSseParser();
		// SSE spec: first space after colon is optional and should be stripped
		const events = parser.push(encode("data:hello\n\n"));
		expect(events).toHaveLength(1);
		expect(events[0].data).toBe("hello");
	});

	it("handles empty data field", () => {
		const parser = createSseParser();
		const events = parser.push(encode("data\n\n"));
		expect(events).toHaveLength(1);
		expect(events[0].data).toBe("");
	});

	it("handles event with no data (just event type)", () => {
		const parser = createSseParser();
		const events = parser.push(encode("event: ping\n\n"));
		expect(events).toHaveLength(1);
		expect(events[0].event).toBe("ping");
		expect(events[0].data).toBe("");
	});
});
