import {
	CopyIcon,
	DownloadIcon,
	EyeClosedIcon,
	EyeIcon,
} from "@primer/octicons-react";
import { Button, Spinner, TextInput } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";

import { serviceApi } from "@/api";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awd/$id/ssh")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const q = useQuery({
		queryKey: ["awd-ssh", id],
		queryFn: () => serviceApi.awd.sshConfig(id),
		retry: false,
	});
	const [showPwd, setShowPwd] = useState(false);
	const [copied, setCopied] = useState<string | null>(null);

	if (q.isLoading) return <Spinner />;

	const copy = async (key: string, text: string) => {
		await navigator.clipboard.writeText(text);
		setCopied(key);
		setTimeout(() => setCopied(null), 1500);
	};

	// 未加入队伍 / 网络未部署时给出引导而非报错。
	if (q.isError || !q.data?.data) {
		return (
			<div className="flex flex-col gap-2 max-w-xl">
				<p className="text-sm opacity-80">
					SSH 凭据在比赛部署后开放，需要先加入队伍（Overview 页）。
				</p>
				<p className="text-sm opacity-80">
					部署完成后此处会显示团队共享密码与每个游戏盒的连接命令。
				</p>
			</div>
		);
	}

	const ssh = q.data.data;
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
				一队一密码（团队共享）：所有游戏盒使用相同 SSH 密码；用户名/IP
				按实例不同。
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
