import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { useAuthStore } from "@/stores/AuthStore";
import { ADMIN_API_URL } from "@/config";

export const Route = createFileRoute("/admin/terminal")({
    component: RouteComponent,
});

function buildWsUrl(): string {
    const token = useAuthStore.getState().adminToken;
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    return `${protocol}//${window.location.host}${ADMIN_API_URL}/terminal/ws?token=${token}`;
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

        const ws = new WebSocket(buildWsUrl());
        wsRef.current = ws;

        ws.onopen = () => {
            terminal.writeln("\x1b[32m● Connected\x1b[0m\r\n");
            // Send initial terminal size immediately
            const { cols, rows } = terminal;
            ws.send(JSON.stringify({ type: "resize", cols, rows }));
        };

        ws.onmessage = (event) => {
            if (event.data instanceof Blob) {
                event.data.arrayBuffer().then((buf) => {
                    terminal.write(new Uint8Array(buf));
                });
            } else {
                terminal.write(event.data);
            }
        };

        ws.onerror = () => {
            terminal.writeln("\r\n\x1b[31m● WebSocket error\x1b[0m");
        };

        ws.onclose = () => {
            terminal.writeln("\r\n\x1b[31m● Connection closed\x1b[0m");
        };

        terminal.onData((data) => {
            if (ws.readyState === WebSocket.OPEN) {
                ws.send(data);
            }
        });

        terminal.onResize(({ cols, rows }) => {
            if (ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({ type: "resize", cols, rows }));
            }
        });

        // Attach terminal to container
        if (containerRef.current) {
            terminal.open(containerRef.current);
            fitAddon.fit();
        }

        // ResizeObserver to keep terminal fitted
        const observer = new ResizeObserver(() => {
            try {
                fitAddon.fit();
            } catch {
                // ignore
            }
        });
        if (containerRef.current) {
            observer.observe(containerRef.current);
        }

        return () => {
            observer.disconnect();
            ws.close();
            terminal.dispose();
        };
    }, []);

    return (
        <div className="h-full bg-[#1e1e1e] overflow-hidden">
            <div ref={containerRef} className="h-full" />
        </div>
    );
}
