import {
	CopyIcon,
	DownloadIcon,
	EyeClosedIcon,
	EyeIcon,
} from "@primer/octicons-react";
import { Button, Spinner, TextInput } from "@primer/react";
import { InlineMessage } from "@primer/react/experimental";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";

import { serviceApi } from "@/api";
import { awdPlayerApi } from "@/api/awd";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awd/$id/ssh")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

/** Get a human-readable reason why SSH is unavailable. */
function sshUnavailableReason(awdPhase: string | undefined, awdStatus: string | undefined, banned: boolean): string | null {
	if (banned) return "Team banned — access unavailable.";
	if (!awdStatus || !awdPhase) return null;
	if (awdStatus === "finished" || awdStatus === "archived") return "Competition finished — SSH locked.";
	if (awdStatus === "paused") return "Competition paused — SSH unavailable.";
	if (awdStatus === "network_error") return "Infrastructure unavailable.";
	if (awdPhase === "pause") return "Competition paused — SSH unavailable.";
	return null;
}

function RouteComponent() {
	const { id } = Route.useParams();
	const q = useQuery({
		queryKey: ["awd-ssh", id],
		queryFn: () => serviceApi.awd.sshConfig(id),
		retry: false,
	});

	const statusQuery = useQuery({
		queryKey: ["awd-player-status", id],
		queryFn: () => awdPlayerApi.status(id),
		retry: false,
	});

	const awdStatus = statusQuery.data?.data ?? null;
	const [showPwd, setShowPwd] = useState(false);
	const [copied, setCopied] = useState<string | null>(null);

	if (q.isLoading || statusQuery.isLoading) return <Spinner />;

	const unavailableReason = sshUnavailableReason(
		awdStatus?.phase,
		awdStatus?.status,
		awdStatus?.banned ?? false,
	);

	const copy = async (key: string, text: string) => {
		await navigator.clipboard.writeText(text);
		setCopied(key);
		setTimeout(() => setCopied(null), 1500);
	};

	// Error or unavailable
	if (q.isError || !q.data?.data) {
		return (
			<div className="flex flex-col gap-2 max-w-xl">
				{unavailableReason ? (
					<InlineMessage variant="warning">
						{unavailableReason}
					</InlineMessage>
				) : (
					<>
						<p className="text-sm opacity-80">
							SSH credentials are available after deployment. Join a team on the Overview page first.
						</p>
						<p className="text-sm opacity-80">
							Once deployed, team-shared password and connection commands for each GameBox will appear here.
						</p>
					</>
				)}
			</div>
		);
	}

	const ssh = q.data.data;

	// Show state warning if SSH shouldn't be available
	if (unavailableReason) {
		return (
			<div className="flex flex-col gap-2 max-w-xl">
				<InlineMessage variant="warning">
					{unavailableReason}
				</InlineMessage>
			</div>
		);
	}

	const download = () => {
		const lines = [
			`FloatCTF AWD SSH Access (event ${id})`,
			`Port: ${ssh.port}`,
			`Password: ${ssh.password}`,
			``,
			...ssh.instances.map(
				(i) =>
					`ssh -p ${ssh.port} ${i.username}@${i.gamebox_ip}  # ${i.container_name}`,
			),
		];
		const blob = new Blob([lines.join("\n")], { type: "text/plain" });
		const url = URL.createObjectURL(blob);
		const a = document.createElement("a");
		a.href = url;
		a.download = `floatctf-awd-${id}-ssh.txt`;
		a.click();
		URL.revokeObjectURL(url);
	};

	return (
		<div className="flex flex-col gap-4 max-w-2xl">
			<div className="flex items-center justify-between">
				<h4 className="font-bold">SSH Access</h4>
				<Button leadingVisual={DownloadIcon} onClick={download}>
					Download ssh-access.txt
				</Button>
			</div>
			<p className="text-xs opacity-70">
				One password per team (shared): all GameBoxes use the same SSH password.
				Username and IP vary per instance.
			</p>

			<div className="flex flex-col gap-1">
				<span className="text-xs font-semibold">Password (team shared)</span>
				<div className="flex gap-2 items-center">
					<TextInput
						value={showPwd ? ssh.password : "••••••••••••••••"}
						readOnly
						onClick={() => setShowPwd((v) => !v)}
						aria-label="SSH password"
						block
					/>
					<Button
						leadingVisual={CopyIcon}
						onClick={() => copy("pwd", ssh.password)}
					>
						{copied === "pwd" ? "Copied" : "Copy"}
					</Button>
					<Button
						leadingVisual={showPwd ? EyeClosedIcon : EyeIcon}
						aria-label={showPwd ? "Hide password" : "Show password"}
						onClick={() => setShowPwd((v) => !v)}
					/>
				</div>
			</div>

			<table className="w-full text-sm">
				<thead>
					<tr>
						<th align="left">IP</th>
						<th align="left">User</th>
						<th align="left">Status</th>
						<th align="left">Command</th>
						<th />
					</tr>
				</thead>
				<tbody>
					{ssh.instances.map((i) => {
						const cmd = `ssh -p ${ssh.port} ${i.username}@${i.gamebox_ip}`;
						return (
							<tr key={i.id}>
								<td>
									<code>{i.gamebox_ip}</code>
								</td>
								<td>{i.username}</td>
								<td>{i.health_status}</td>
								<td>
									<code className="text-xs">{cmd}</code>
								</td>
								<td>
									<Button
										size="small"
										leadingVisual={CopyIcon}
										onClick={() => copy(i.id, cmd)}
									>
										{copied === i.id ? "Copied" : "Copy"}
									</Button>
								</td>
							</tr>
						);
					})}
					{ssh.instances.length === 0 && (
						<tr>
							<td colSpan={5}>No GameBoxes deployed for your team yet.</td>
						</tr>
					)}
				</tbody>
			</table>
		</div>
	);
}