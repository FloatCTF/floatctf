import { CopyIcon, DownloadIcon } from "@primer/octicons-react";
import { Button, Spinner } from "@primer/react";
import { InlineMessage } from "@primer/react/experimental";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";

import { serviceApi } from "@/api";
import { awdPlayerApi } from "@/api/awd";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awd/$id/wireguard")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

/** Get a human-readable reason why WireGuard is unavailable. */
function wgUnavailableReason(awdPhase: string | undefined, awdStatus: string | undefined, banned: boolean, finalSettlement: boolean): string | null {
	if (banned) return "Team banned — access unavailable.";
	if (!awdStatus || !awdPhase) return null;
	if (awdStatus === "finished" || awdStatus === "archived") return "Competition finished — access locked.";
	if (finalSettlement) return "Final settlement — competition access is closed.";
	if (awdStatus === "paused") return "Competition paused — access unavailable.";
	if (awdStatus === "network_error") return "Infrastructure unavailable.";
	if (awdPhase === "pause") return "Competition paused — access unavailable.";
	return null;
}

function RouteComponent() {
	const { id } = Route.useParams();
	const q = useQuery({
		queryKey: ["awd-wg", id],
		queryFn: () => serviceApi.awd.wireguardConfig(id),
		retry: false,
	});

	const statusQuery = useQuery({
		queryKey: ["awd-player-status", id],
		queryFn: () => awdPlayerApi.status(id),
		retry: false,
	});

	const awdStatus = statusQuery.data?.data ?? null;
	const [copied, setCopied] = useState(false);

	const conf = q.data?.data?.config ?? "";

	const download = () => {
		const blob = new Blob([conf], { type: "text/plain" });
		const url = URL.createObjectURL(blob);
		const a = document.createElement("a");
		a.href = url;
		a.download = `floatctf-awd-${id}.conf`;
		a.click();
		URL.revokeObjectURL(url);
	};

	const copy = async () => {
		await navigator.clipboard.writeText(conf);
		setCopied(true);
		setTimeout(() => setCopied(false), 1500);
	};

	if (q.isLoading || statusQuery.isLoading) return <Spinner />;

	const unavailableReason = wgUnavailableReason(
		awdStatus?.phase,
		awdStatus?.status,
		awdStatus?.banned ?? false,
		awdStatus?.final_settlement ?? false,
	);

	if (q.isError || !conf) {
		return (
			<div className="flex flex-col gap-2 max-w-xl">
				{unavailableReason ? (
					<InlineMessage variant="warning">
						{unavailableReason}
					</InlineMessage>
				) : (
					<>
						<p className="text-sm opacity-80">
							Unable to retrieve WireGuard configuration. Common reasons:
						</p>
						<ul className="list-disc pl-6 text-sm opacity-80">
							<li>Not joined a team (join on Overview page first)</li>
							<li>Event not yet deployed (wait for admin to Deploy / Start)</li>
							<li>
								Private key is returned only on first fetch; if previously downloaded,
								contact admin to rotate keys
							</li>
						</ul>
					</>
				)}
			</div>
		);
	}

	// Show state warning if WG shouldn't be available
	if (unavailableReason) {
		return (
			<div className="flex flex-col gap-2 max-w-xl">
				<InlineMessage variant="warning">
					{unavailableReason}
				</InlineMessage>
			</div>
		);
	}

	return (
		<div className="flex flex-col gap-3 max-w-2xl">
			<div className="flex items-center justify-between">
				<h4 className="font-bold">WireGuard VPN</h4>
				<div className="flex gap-2">
					<Button leadingVisual={CopyIcon} onClick={copy}>
						{copied ? "Copied" : "Copy .conf"}
					</Button>
					<Button leadingVisual={DownloadIcon} onClick={download}>
						Download .conf
					</Button>
				</div>
			</div>
			<p className="text-xs opacity-70">
				Connect to VPN to access GameBox internal network. Private key is returned
				only on first fetch — save it securely. If lost, contact admin to rotate keys.
			</p>
			<pre className="p-3 bg-canvas-subtle overflow-auto text-xs rounded">
				{conf}
			</pre>
			<div className="text-sm opacity-80">
				<p className="font-semibold mb-1">Usage</p>
				<ul className="list-disc pl-6 flex flex-col gap-1">
					<li>
						<strong>Windows / macOS</strong>: Install WireGuard client → Import
						tunnel(s) from file → select downloaded .conf
					</li>
					<li>
						<strong>Linux</strong>:{" "}
						<code>sudo wg-quick up ./floatctf-awd-{id}.conf</code>
					</li>
					<li>
						<strong>Android / iOS</strong>: Official WireGuard app — scan QR or import .conf
					</li>
				</ul>
			</div>
		</div>
	);
}