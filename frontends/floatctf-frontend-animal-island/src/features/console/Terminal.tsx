/**
 * Web 终端（xterm.js over WebSocket）。
 *
 * ## 为什么这里需要三处"非 SDK"代码，以及它们为什么是安全的
 *
 * PUBLIC SDK GAP: Web 终端未纳入 `@floatctf/sdk`（`client.admin` 下没有 terminal 模块）。
 * 平台提供的是两步握手，而它**恰好就是为了不把会话令牌放进 URL** 而设计的：
 *
 *   1. `POST /api/admin/terminal/session`
 *      —— 用常规 `Authorization: Bearer <session token>` 换取一个
 *      **60 秒有效、单次使用、HttpOnly、SameSite=Strict、path 限定为
 *      `/api/admin/terminal/ws`** 的 ticket cookie。
 *   2. `GET /api/admin/terminal/ws`（WebSocket upgrade）
 *      —— 只用 **ticket cookie** 认证，URL 上不带任何凭据。
 *
 * 因此本文件里的 `fetch`/`WebSocket` 是**刻意的**：SDK 的传输层是 axios，
 * 而浏览器不允许为 WebSocket 设置自定义头 —— 令牌只能靠 cookie 过桥。
 * **绝不**把 token 拼进 WS 的 query string（那会进入访问日志、Referer 与
 * 浏览器历史）。这里的实现完全没有这样做。
 *
 * WS 协议（见后端 `terminal.rs`）：
 *   - 文本帧：若可解析为 `{"type":"resize","cols":N,"rows":M}` 则调整 PTY 尺寸，
 *     否则视为**标准输入**；二进制帧一律视为标准输入；
 *   - 服务端 → 客户端：shell 输出文本帧。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { Button, Card, Divider, Tag } from "animal-island-ui";
import {
	AlertTriangleIcon,
	CheckCircleIcon,
	MonitorIcon,
	LinkIcon,
	RefreshIcon,
	TrashIcon,
	UnlinkIcon,
} from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { PageHeader } from "~/app/shell";
import { useConfirm } from "~/components/confirm";
import { currentToken } from "~/auth/store";

type Phase = "idle" | "issuing" | "connecting" | "connected" | "closed" | "error";

const PHASE_META: Record<Phase, { label: string; color: string }> = {
	idle: { label: "未连接", color: "brown" },
	issuing: { label: "申请票据…", color: "app-yellow" },
	connecting: { label: "建立连接…", color: "app-yellow" },
	connected: { label: "已连接", color: "app-green" },
	closed: { label: "连接已断开", color: "brown" },
	error: { label: "连接失败", color: "app-red" },
};

/** 终端主题：使用与全站一致的暖色暗调（终端是设计法里等宽/暗色的例外场景）。 */
const TERMINAL_THEME = {
	background: "#3a3229",
	foreground: "#f8f8f0",
	cursor: "#19c8b9",
	selectionBackground: "#19c8b955",
} as const;

export function ConsoleTerminal() {
	const runtime = useRuntime();
	const confirm = useConfirm();

	const hostRef = useRef<HTMLDivElement | null>(null);
	const termRef = useRef<XTerm | null>(null);
	const fitRef = useRef<FitAddon | null>(null);
	const socketRef = useRef<WebSocket | null>(null);

	const [phase, setPhase] = useState<Phase>("idle");
	const [message, setMessage] = useState<string>("");

	/** 断开连接（幂等）。 */
	const disconnect = useCallback(() => {
		const sock = socketRef.current;
		socketRef.current = null;
		if (sock) {
			sock.onopen = null;
			sock.onmessage = null;
			sock.onerror = null;
			sock.onclose = null;
			try {
				sock.close(1000, "client disconnect");
			} catch {
				// 已经关掉了，无所谓
			}
		}
	}, []);

	// xterm 实例与容器同生命周期。
	useEffect(() => {
		if (!hostRef.current) return;
		const term = new XTerm({
			cursorBlink: true,
			fontSize: 13,
			fontFamily:
				'"JetBrains Mono", "Fira Code", ui-monospace, SFMono-Regular, Menlo, monospace',
			theme: TERMINAL_THEME,
			scrollback: 5000,
			convertEol: false,
		});
		const fit = new FitAddon();
		term.loadAddon(fit);
		term.open(hostRef.current);
		fit.fit();
		termRef.current = term;
		fitRef.current = fit;

		// 用户输入 → WS。未连接时提示而不是静默丢弃 —— 静默丢弃会让人以为终端卡死。
		const dataSub = term.onData((data) => {
			const sock = socketRef.current;
			if (sock && sock.readyState === WebSocket.OPEN) {
				sock.send(data);
			}
		});

		const syncSize = () => {
			try {
				fit.fit();
			} catch {
				return;
			}
			const sock = socketRef.current;
			if (sock && sock.readyState === WebSocket.OPEN) {
				sock.send(
					JSON.stringify({ type: "resize", cols: term.cols, rows: term.rows }),
				);
			}
		};
		window.addEventListener("resize", syncSize);
		// 容器尺寸变化时也要重新 fit（Tabs 切换等）。
		const observer = new ResizeObserver(syncSize);
		observer.observe(hostRef.current);

		return () => {
			window.removeEventListener("resize", syncSize);
			observer.disconnect();
			dataSub.dispose();
			disconnect();
			term.dispose();
			termRef.current = null;
			fitRef.current = null;
		};
	}, [disconnect]);

	async function connect() {
		const term = termRef.current;
		if (!term) return;

		disconnect();
		setMessage("");
		term.clear();

		const token = currentToken();
		if (!token) {
			setPhase("error");
			setMessage("当前没有会话令牌，无法申请终端票据。");
			return;
		}

		// ── 第 1 步：用常规 Bearer 头换取一次性 ticket cookie ──
		setPhase("issuing");
		try {
			const res = await fetch(`${runtime.apiBaseUrl}/admin/terminal/session`, {
				method: "POST",
				headers: { Authorization: `Bearer ${token}` },
				// 必须带凭据，否则 Set-Cookie 不会被保存，WS 阶段会 401。
				credentials: "include",
			});
			if (res.status === 404) {
				setPhase("error");
				setMessage("平台关闭了 Web 终端功能（后端返回 404）。");
				return;
			}
			if (res.status === 401) {
				setPhase("error");
				setMessage("会话已失效，请重新登录。");
				return;
			}
			if (res.status === 403) {
				setPhase("error");
				setMessage("你的账号没有 terminal.open 权限。");
				return;
			}
			if (!res.ok) {
				setPhase("error");
				setMessage(`申请终端票据失败：HTTP ${res.status}`);
				return;
			}
		} catch (e) {
			setPhase("error");
			setMessage(`申请终端票据时网络错误：${e instanceof Error ? e.message : String(e)}`);
			return;
		}

		// ── 第 2 步：升级为 WebSocket，只靠 cookie 认证 ──
		setPhase("connecting");
		const wsBase = runtime.apiBaseUrl.replace(/^http/i, (m) => (m.toLowerCase() === "http" ? "ws" : "wss"));
		let sock: WebSocket;
		try {
			// 注意：URL 上**没有任何凭据**（ticket 走 HttpOnly cookie）。
			sock = new WebSocket(`${wsBase}/admin/terminal/ws`);
		} catch (e) {
			setPhase("error");
			setMessage(`无法建立 WebSocket：${e instanceof Error ? e.message : String(e)}`);
			return;
		}
		socketRef.current = sock;
		sock.binaryType = "arraybuffer";

		sock.onopen = () => {
			setPhase("connected");
			term.focus();
			const fit = fitRef.current;
			if (fit) {
				try {
					fit.fit();
				} catch {
					// 忽略：尺寸上报失败不影响会话
				}
			}
			sock.send(JSON.stringify({ type: "resize", cols: term.cols, rows: term.rows }));
		};

		sock.onmessage = (event) => {
			if (typeof event.data === "string") {
				term.write(event.data);
			} else if (event.data instanceof ArrayBuffer) {
				term.write(new Uint8Array(event.data));
			}
		};

		sock.onerror = () => {
			// onerror 不提供细节（浏览器安全限制），真正的信息在 onclose。
			setPhase("error");
			setMessage("WebSocket 发生错误。常见原因：票据已过期（60 秒）、被重复使用，或代理未转发 WS。");
		};

		sock.onclose = (event) => {
			socketRef.current = null;
			// 正常关闭（1000）不算错误；其余都给出原因。
			if (event.code === 1000) {
				setPhase("closed");
				setMessage("终端会话已关闭。");
			} else {
				setPhase("closed");
				setMessage(
					`连接断开（code ${event.code}${event.reason ? `：${event.reason}` : ""}）。` +
						"如果这一步是 401，说明 ticket 已用过或超过 60 秒 —— 重新连接即可。",
				);
			}
			term.write("\r\n\x1b[33m[会话已结束]\x1b[0m\r\n");
		};
	}

	const meta = PHASE_META[phase];
	const connected = phase === "connected";

	return (
		<>
			<PageHeader
				title="Web 终端"
				description="在平台上直接操作 API 所在容器。凭据通过一次性 HttpOnly 票据过桥，不出现在 URL 上。"
				extra={
					<>
						<Tag size="small" variant="soft" color={meta.color as "app-green"}>
							{meta.label}
						</Tag>
						{connected ? (
							<Button
								size="small"
								danger
								icon={<UnlinkIcon size={16} />}
								onClick={async () => {
									const ok = await confirm({
										title: "断开终端会话",
										consequence:
											"将关闭 WebSocket 并终止这个 shell。**shell 里未保存的工作与后台进程会一起结束**（进程随会话销毁）。",
										okText: "断开",
										danger: true,
									});
									if (!ok) return;
									disconnect();
									setPhase("closed");
									setMessage("已手动断开。");
								}}
							>
								断开
							</Button>
						) : (
							<Button
								type="primary"
								icon={<LinkIcon size={16} />}
								loading={phase === "issuing" || phase === "connecting"}
								onClick={() => void connect()}
							>
								连接终端
							</Button>
						)}
						<Button
							size="small"
							icon={<TrashIcon size={16} />}
							onClick={() => termRef.current?.clear()}
						>
							清屏
						</Button>
					</>
				}
			/>

			<Card>
				<div className="ai-row" style={{ gap: 8 }}>
					{phase === "error" || (phase === "closed" && message) ? (
						<AlertTriangleIcon size={16} />
					) : connected ? (
						<CheckCircleIcon size={16} />
					) : (
						<MonitorIcon size={16} />
					)}
					<span className="ai-muted">
						{message ||
							"点击「连接终端」开始。终端内的输入会直接执行在 API 容器里 —— 这是宿主级能力，请谨慎。"}
					</span>
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 10 }}>
					<div className="ai-row-between">
						<span className="ai-strong">会话</span>
						<Tag size="small" variant="outlined">
							xterm-256color
						</Tag>
					</div>
					<Divider />
					{/* 终端是设计法里"等宽/暗色"的明确例外场景。 */}
					<div
						ref={hostRef}
						className="ai-terminal-host"
						style={{ height: 460, width: "100%" }}
					/>
				</div>
			</Card>

			<Card>
				<div className="ai-col" style={{ gap: 6 }}>
					<span className="ai-strong">安全说明（为什么 URL 上是干净的）</span>
					<span className="ai-muted">
						连接分两步：先用 `Authorization` 头换取**60 秒单次使用**的 HttpOnly ticket，
						再用它升级 WebSocket。会话令牌**从不**出现在 URL、query string 或浏览器历史里。
						票据的 path 被后端限定为 `/api/admin/terminal/ws`，其它请求带不上它。
					</span>
					<span className="ai-muted">
						另外：升级前后端会**重新解析一次权限**并现查 `terminal.open`。
						也就是说，即使票据是在权限被撤销之前签发的，也会被拒绝 ——
						终端是宿主级能力，不能靠"发票据那一刻有权限"就一直有效。
					</span>
					<div className="ai-row" style={{ gap: 8 }}>
						<RefreshIcon size={16} />
						<span className="ai-muted" style={{ fontSize: 12 }}>
							如果连接总在 401 失败，通常是反向代理没有转发 WebSocket 升级头。
						</span>
					</div>
				</div>
			</Card>
		</>
	);
}
