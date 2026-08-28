/**
 * SSE 帧解析器。
 *
 * 正确处理 SSE 协议帧边界：
 * - 事件以空行分隔
 * - 多个 `data:` 行以 "\n" 拼接
 * - 支持 `event:` `id:` `retry:` 字段
 * - 以 `:` 开头的行为注释，不派发事件
 * - 不完整帧在跨 chunk 边界时保留
 * - 支持 CRLF 和 LF 行尾
 * - UTF-8 增量解码（TextDecoder stream 模式）
 */

export interface SseEvent {
	/** 事件类型；未指定时为空字符串（SSE 默认 message） */
	event: string;
	/** 事件数据（多个 data: 行以 \n 拼接） */
	data: string;
	/** 事件 ID（可选） */
	id?: string;
	/** 重连间隔（毫秒，可选） */
	retry?: number;
}

export interface SseParser {
	/** 推送原始字节块，返回解析出的完整事件 */
	push(chunk: Uint8Array): SseEvent[];
	/** 清空内部状态 */
	reset(): void;
}

/**
 * 创建 SSE 解析器。
 * 使用 TextDecoder stream 模式处理跨 chunk 边界的 UTF-8 字节。
 */
export function createSseParser(): SseParser {
	const decoder = new TextDecoder("utf-8", { fatal: false });
	let buffer = "";

	function* parseLines(): Generator<SseEvent> {
		let eventType = "";
		let dataLines: string[] = [];
		let eventId: string | undefined;
		let retry: number | undefined;

		const lines = buffer.split(/\r?\n/);
		// 最后一行可能不完整（没有 \n），保留
		buffer = lines.pop() ?? "";

		for (const rawLine of lines) {
			const line = rawLine.replace(/\r$/, "");

			// 空行 → 事件边界
			if (line === "") {
				if (dataLines.length > 0 || eventType !== "") {
					yield {
						event: eventType,
						data: dataLines.join("\n"),
						id: eventId,
						retry,
					};
				}
				eventType = "";
				dataLines = [];
				eventId = undefined;
				retry = undefined;
				continue;
			}

			// 注释行（以 : 开头）→ 忽略
			if (line.startsWith(":")) {
				continue;
			}

			// 解析字段
			const colonIdx = line.indexOf(":");
			if (colonIdx === -1) {
				// 无冒号 → 整个行是字段名，值为空
				const field = line;
				if (field === "data") {
					dataLines.push("");
				}
				// 忽略未知字段
				continue;
			}

			const field = line.slice(0, colonIdx);
			let value = line.slice(colonIdx + 1);
			// 去掉前导空格（SSE 规范）
			if (value.startsWith(" ")) {
				value = value.slice(1);
			}

			switch (field) {
				case "data":
					dataLines.push(value);
					break;
				case "event":
					eventType = value;
					break;
				case "id":
					eventId = value;
					break;
				case "retry":
					{
						const ms = Number.parseInt(value, 10);
						if (!Number.isNaN(ms) && ms > 0) {
							retry = ms;
						}
					}
					break;
				// 未知字段：忽略
			}
		}
	}

	return {
		push(chunk: Uint8Array): SseEvent[] {
			const text = decoder.decode(chunk, { stream: true });
			buffer += text;
			return [...parseLines()];
		},
		reset() {
			buffer = "";
		},
	};
}
