/**
 * 管理端 · Web 终端（`/admin/infra/terminal`，specialized 能力）。
 *
 * ## 逃生舱声明（PUBLIC SDK GAP · class B）
 *
 * 终端**没有** SDK 抽象，必须走逃生舱（SDK 只提供 HTTP / SSE 门面）：
 * 1. `await client.adminHttp.post("/terminal/session")`
 *    → 后端 `POST /api/admin/terminal/session` 用**一次性** short-lived ticket 签发
 *      HttpOnly cookie（`path=/api/admin/terminal/ws`、SameSite=Strict、60s）；
 *    **token 绝不进 URL**：常规超管 JWT 只通过 Authorization 头发这一次。
 * 2. `new WebSocket(<ws url>)` → 后端 `GET /api/admin/terminal/ws`（actix-ws），
 *    ticket 由 cookie 携带并在升级前原子消费（一次性）。
 *    WS URL 由 `client.adminBaseUrl` 派生（http(s) → ws(s)，路径改为 `/terminal/ws`），
 *    与 Default `routes/admin/terminal.tsx` 的语义一致。
 *
 * 协议：文本帧 = 终端输入 / 后端输出；客户端还会发 `{"type":"resize","cols","rows"}`；
 * 后端可能下发二进制帧，因此 `binaryType = "arraybuffer"` 并按 Uint8Array 写入 xterm。
 *
 * 失败态：功能由 TOML `[features].enable_web_terminal` 门控，关闭时 `POST /terminal/session`
 * 返回 404；后端文案（纯文本 body）会原样显示在页面上，并给出重连入口。
 */

import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import { PlugZap, RotateCw, ShieldAlert, TerminalSquare } from "lucide-react";
import "@xterm/xterm/css/xterm.css";

import { FloatCTFError } from "@floatctf/sdk";

import { useClient } from "~/api/client";
import { errorText } from "~/api/errors";
import { TonePill, type PillTone } from "~/components/app/badges";
import { PageBody, PageHeader, SectionCard, Toolbar } from "~/components/app/page";
import { Button } from "~/components/ui/button";
import { useDocumentTitle } from "~/lib/hooks";

type TerminalStatus =
	| "idle"
	| "authorizing"
	| "connecting"
	| "connected"
	| "closed"
	| "error";

const STATUS_META: Record<TerminalStatus, { tone: PillTone; text: string }> = {
	idle: { tone: "muted", text: "未连接" },
	authorizing: { tone: "info", text: "申请终端票据…" },
	connecting: { tone: "info", text: "建立 WebSocket…" },
	connected: { tone: "success", text: "已连接" },
	closed: { tone: "warning", text: "连接已断开" },
	error: { tone: "danger", text: "连接失败" },
};

/** 从 `client.adminBaseUrl` 派生 WS URL：http(s) → ws(s)，路径加 `/terminal/ws`。 */
export function buildTerminalWsUrl(adminBaseUrl: string, origin: string): string {
	const base = new URL(adminBaseUrl, origin);
	const protocol = base.protocol === "https:" ? "wss:" : "ws:";
	const path = `${base.pathname.replace(/\/+$/, "")}/terminal/ws`;
	return `${protocol}//${base.host}${path}`;
}

/**
 * 后端错误文案：actix 的 `ErrorNotFound("Web terminal is disabled")` 是**纯文本** body，
 * 平台错误模型只解析 JSON `message`，因此这里额外把纯文本 body 附上。
 */
function describeTerminalError(error: unknown): string {
	const base = errorText(error, "终端连接失败");
	if (error instanceof FloatCTFError) {
		const data = error.response?.data;
		if (typeof data === "string" && data.trim().length > 0) {
			return `${base} —— 后端：${data.trim()}`;
		}
		if (error.httpStatus === 404) {
			return `${base}（终端功能可能未在 TOML 打开 [features].enable_web_terminal）`;
		}
	}
	return base;
}

export function AdminTerminalPage(): ReactNode {
	useDocumentTitle("Web 终端 · FloatCTF 控制台");
	const client = useClient();
	const containerRef = useRef<HTMLDivElement>(null);
	const [status, setStatus] = useState<TerminalStatus>("idle");
	const [error, setError] = useState<string | null>(null);
	const [attempt, setAttempt] = useState(0);

	const adminBaseUrl = client.adminBaseUrl;

	const connectEffect = useCallback(() => {
		const container = containerRef.current;
		if (!container) return;

		let disposed = false;
		const terminal = new Terminal({
			cursorBlink: true,
			cursorStyle: "bar",
			fontSize: 14,
			fontFamily: 'Menlo, Monaco, "Courier New", monospace',
			allowProposedApi: true,
			theme: {
				background: "#0b0b0d",
				foreground: "#e4e4e7",
				cursor: "#e4e4e7",
				selectionBackground: "#3f3f46",
			},
		});
		const fitAddon = new FitAddon();
		terminal.loadAddon(fitAddon);

		let socket: WebSocket | null = null;
		let observer: ResizeObserver | null = null;
		let initializeTimer = 0;
		let fitFrame: number | null = null;

		const sendResize = () => {
			if (socket?.readyState === WebSocket.OPEN) {
				socket.send(JSON.stringify({ type: "resize", cols: terminal.cols, rows: terminal.rows }));
			}
		};

		const dataDisposable = terminal.onData((data) => {
			if (socket?.readyState === WebSocket.OPEN) socket.send(data);
		});
		const resizeDisposable = terminal.onResize(() => sendResize());

		function writeLine(text: string, tone: "info" | "error" | "muted" = "muted") {
			const color = tone === "error" ? "\x1b[31m" : tone === "info" ? "\x1b[32m" : "\x1b[90m";
			terminal.writeln(`\r\n${color}● ${text}\x1b[0m`);
		}

		async function connect() {
			setStatus("authorizing");
			setError(null);
			try {
				// 一次性 HttpOnly ticket cookie；JWT 只在 Authorization 头里出现一次。
				await client.adminHttp.post("/terminal/session");
			} catch (sessionError) {
				if (disposed) return;
				const message = describeTerminalError(sessionError);
				setStatus("error");
				setError(message);
				writeLine(message, "error");
				return;
			}
			if (disposed) return;

			setStatus("connecting");
			const url = buildTerminalWsUrl(adminBaseUrl, window.location.origin);
			let ws: WebSocket;
			try {
				ws = new WebSocket(url);
			} catch (socketError) {
				if (disposed) return;
				const message = describeTerminalError(socketError);
				setStatus("error");
				setError(message);
				writeLine(message, "error");
				return;
			}
			ws.binaryType = "arraybuffer";
			socket = ws;

			ws.onopen = () => {
				if (disposed) return;
				setStatus("connected");
				writeLine("已连接到平台 Web 终端（ticket 一次性消费）", "info");
				sendResize();
			};

			ws.onmessage = (event) => {
				if (disposed) return;
				if (event.data instanceof ArrayBuffer) {
					terminal.write(new Uint8Array(event.data));
					return;
				}
				if (event.data instanceof Blob) {
					void event.data.arrayBuffer().then((buffer) => {
						if (!disposed) terminal.write(new Uint8Array(buffer));
					});
					return;
				}
				if (typeof event.data === "string") terminal.write(event.data);
			};

			ws.onerror = () => {
				if (disposed) return;
				setStatus("error");
				setError("WebSocket 连接异常（票据可能已过期或被拒绝）");
				writeLine("WebSocket error", "error");
			};

			ws.onclose = (event) => {
				if (disposed) return;
				setStatus((previous) => (previous === "error" ? previous : "closed"));
				writeLine(
					event.code === 1000
						? "连接已关闭"
						: `连接已关闭（code=${event.code}${event.reason ? ` ${event.reason}` : ""}）`,
					event.code === 1000 ? "muted" : "error",
				);
				if (socket === ws) socket = null;
			};
		}

		// xterm 的 Viewport.open() 会注册不可取消的延迟回调：StrictMode 探测时若同步
		// open 后立刻 dispose 会访问已释放的 renderService，因此把 open 推迟一个 turn。
		initializeTimer = window.setTimeout(() => {
			if (disposed) return;
			terminal.open(container);
			observer = new ResizeObserver(() => {
				if (disposed) return;
				try {
					fitAddon.fit();
				} catch {
					// 容器在切换 / 卸载边界上的尺寸变化无需影响连接。
				}
			});
			observer.observe(container);

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
			dataDisposable.dispose();
			resizeDisposable.dispose();
			if (socket) {
				socket.onopen = null;
				socket.onmessage = null;
				socket.onerror = null;
				socket.onclose = null;
				socket.close();
				socket = null;
			}
			terminal.dispose();
			setStatus("idle");
		};
	}, [adminBaseUrl, client]);

	useEffect(() => {
		const cleanup = connectEffect();
		return cleanup;
		// attempt 变化时整体重连（先 dispose 旧终端 / socket）。
	}, [connectEffect, attempt]);

	const meta = STATUS_META[status];

	return (
		<PageBody>
			<PageHeader
				title="Web 终端"
				description="在宿主 helper 上打开的交互式 shell（fish > zsh > bash > sh）。鉴权用一次性 HttpOnly 票据，token 不进入 URL。"
				badge={<TonePill tone="muted">specialized</TonePill>}
				actions={
					<Toolbar>
						<TonePill tone={meta.tone} icon={<PlugZap />}>
							{meta.text}
						</TonePill>
						<Button
							variant="outline"
							size="sm"
							onClick={() => setAttempt((value) => value + 1)}
							disabled={status === "authorizing" || status === "connecting"}
						>
							<RotateCw />
							重新连接
						</Button>
					</Toolbar>
				}
			/>

			<div className="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm">
				<ShieldAlert className="mt-0.5 size-4 shrink-0 text-destructive" />
				<span>
					终端直接落在宿主环境：命令以宿主权限执行，且会真实修改容器 / 网络 / 文件。
					关闭页面或点「重新连接」会结束当前会话（票据一次性，不能复用）。
				</span>
			</div>

			{error ? (
				<div
					className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-destructive/40 bg-destructive/5 px-3 py-2 text-sm"
					role="alert"
				>
					<span className="text-destructive">{error}</span>
					<Button variant="outline" size="sm" onClick={() => setAttempt((value) => value + 1)}>
						<RotateCw />
						重试
					</Button>
				</div>
			) : null}

			<SectionCard
				title={
					<span className="inline-flex items-center gap-2">
						<TerminalSquare className="size-4" />
						交互终端
					</span>
				}
				description={`WS 端点：${adminBaseUrl.replace(/\/+$/, "")}/terminal/ws（由 adminBaseUrl 派生）`}
				contentClassName="p-0"
			>
				<div className="h-[68vh] min-h-80 bg-[#0b0b0d] p-2">
					<div ref={containerRef} className="h-full w-full" />
				</div>
			</SectionCard>
		</PageBody>
	);
}
