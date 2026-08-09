import { CopyIcon, DownloadIcon } from "@primer/octicons-react";
import { Button, Spinner } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";

import { serviceApi } from "@/api";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awd/$id/wireguard")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const q = useQuery({
		queryKey: ["awd-wg", id],
		queryFn: () => serviceApi.awd.wireguardConfig(id),
		retry: false,
	});
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

	if (q.isLoading) return <Spinner />;
	if (q.isError || !conf) {
		return (
			<div className="flex flex-col gap-2 max-w-xl">
				<p className="text-sm opacity-80">
					未获取到 WireGuard 配置。常见原因：
				</p>
				<ul className="list-disc pl-6 text-sm opacity-80">
					<li>未加入队伍（Overview 页先加入队伍）</li>
					<li>赛事尚未部署（等待管理员 Deploy / Start）</li>
					<li>
						WireGuard 私钥仅首次拉取返回；若之前已下载过，
						需联系管理员轮换密钥才能重新获取
					</li>
				</ul>
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
				连接 VPN
				后才能访问游戏盒内网。私钥仅首次拉取返回，请妥善保存；若丢失需管理员轮换密钥。
			</p>
			<pre className="p-3 bg-canvas-subtle overflow-auto text-xs rounded">
				{conf}
			</pre>
			<div className="text-sm opacity-80">
				<p className="font-semibold mb-1">使用方式</p>
				<ul className="list-disc pl-6 flex flex-col gap-1">
					<li>
						<strong>Windows / macOS</strong>：安装 WireGuard 客户端 → Import
						tunnel(s) from file → 选择下载的 .conf
					</li>
					<li>
						<strong>Linux</strong>：{" "}
						<code>sudo wg-quick up ./floatctf-awd-{id}.conf</code>
					</li>
					<li>
						<strong>Android / iOS</strong>：官方 WireGuard App 扫码或导入 .conf
					</li>
				</ul>
			</div>
		</div>
	);
}
