import { createFileRoute } from "@tanstack/react-router";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import { useEffect, useRef } from "react";
import "@xterm/xterm/css/xterm.css";
import { admin_api } from "@/api/axios";
import { ADMIN_API_URL } from "@/config";

export const Route = createFileRoute("/admin/terminal")({
	component: RouteComponent,
});

function buildWsUrl(): string {
	const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
	return `${protocol}//${window.location.host}${ADMIN_API_URL}/terminal/ws`;
}

function RouteComponent() {
	const containerRef = useRef<HTMLDivElement>(null);
	const terminalRef = useRef<Terminal | null>(null);
	const fitAddonRef = useRef<FitAddon | null>(null);
	const wsRef = useRef<WebSocket | null>(null);

	useEffect(() => {
		const terminal = new Terminal({
			cursorBlink: true,
			cursorStyle: "bar",
			fontSize: 14,
			fontFamily: 'Menlo, Monaco, "Courier New", monospace',
			theme: {
				background: "#1e1e1e",
				foreground: "#d4d4d4",
				cursor: "#d4d4d4",
				selectionBackground: "#264f78",
			},
			allowProposedApi: true,
		});

		const fitAddon = new FitAddon();
		terminal.loadAddon(fitAddon);

		terminalRef.current = terminal;
		fitAddonRef.current = fitAddon;

		let disposed = false;

		const connect = async () => {
			try {
				// 常规管理员 JWT 只通过 Authorization header 使用一次，后端签发
				// 60 秒、单次消费的 HttpOnly terminal ticket cookie。
				await admin_api.post("/terminal/session");
				if (disposed) return;

				const ws = new WebSocket(buildWsUrl());
				wsRef.current = ws;

				ws.onopen = () => {
					terminal.writeln("\x1b[32m● Connected\x1b[0m\r\n");
					const { cols, rows } = terminal;
					ws.send(JSON.stringify({ type: "resize", cols, rows }));
				};

				ws.onmessage = (event) => {
					if (event.data instanceof Blob) {
						event.data.arrayBuffer().then((buf) => {
							if (!disposed) terminal.write(new Uint8Array(buf));
						});
					} else if (!disposed) {
						terminal.write(event.data);
					}
				};

				ws.onerror = () => {
					if (!disposed) {
						terminal.writeln("\r\n\x1b[31m● WebSocket error\x1b[0m");
					}
				};

				ws.onclose = () => {
					if (!disposed) {
						terminal.writeln("\r\n\x1b[31m● Connection closed\x1b[0m");
					}
					if (wsRef.current === ws) wsRef.current = null;
				};
			} catch {
				if (!disposed) {
					terminal.writeln(
						"\r\n\x1b[31m● Failed to authorize terminal session\x1b[0m",
					);
				}
			}
		};

		terminal.onData((data) => {
			const ws = wsRef.current;
			if (ws?.readyState === WebSocket.OPEN) {
				ws.send(data);
			}
		});

		terminal.onResize(({ cols, rows }) => {
			const ws = wsRef.current;
			if (ws?.readyState === WebSocket.OPEN) {
				ws.send(JSON.stringify({ type: "resize", cols, rows }));
			}
		});

		let observer: ResizeObserver | null = null;
		let fitFrame: number | null = null;

		// xterm 的 Viewport.open() 内部会注册不可取消的 setTimeout(syncScrollArea)。
		// React StrictMode 在开发环境会立即执行一次 mount→cleanup→mount 探测；若首个
		// effect 同步 open 后马上 dispose，xterm 的延迟回调会访问已释放的 renderService。
		// 把真正的 open 推迟一个 event-loop turn，cleanup 可在 StrictMode 探测阶段先取消它。
		const initializeTimer = window.setTimeout(() => {
			if (disposed || !containerRef.current) return;

			terminal.open(containerRef.current);
			observer = new ResizeObserver(() => {
				if (disposed) return;
				try {
					fitAddon.fit();
				} catch {
					// 容器切换/卸载边界上的尺寸变化无需影响终端连接。
				}
			});
			observer.observe(containerRef.current);

			// 再等一帧让字符尺寸与父容器布局稳定后 fit，然后授权并建立 WebSocket。
			fitFrame = window.requestAnimationFrame(() => {
				fitFrame = null;
				if (disposed) return;
				try {
					fitAddon.fit();
				} catch {
					// ResizeObserver 后续仍会在尺寸就绪时重试。
				}
				void connect();
			});
		}, 0);

		return () => {
			disposed = true;
			window.clearTimeout(initializeTimer);
			if (fitFrame !== null) window.cancelAnimationFrame(fitFrame);
			observer?.disconnect();
			wsRef.current?.close();
			wsRef.current = null;
			terminal.dispose();
		};
	}, []);

	return (
		<div className="h-full bg-[#1e1e1e] overflow-hidden">
			<div ref={containerRef} className="h-full" />
		</div>
	);
}
