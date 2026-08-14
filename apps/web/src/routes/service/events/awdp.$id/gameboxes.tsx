import { createFileRoute } from "@tanstack/react-router";

import { ServiceRouteGuard } from "../../route";
import { AwdpEventWorkbench } from "./-workbench";

export const Route = createFileRoute("/service/events/awdp/$id/gameboxes")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

/**
 * GameBoxes tab：直接复用共享 <AwdpWorkbench>（§65），
 * 与 Overview 页同一套 VM 适配器，不复制两套页面。
 */
function RouteComponent() {
	const { id } = Route.useParams();
	return (
		<div className="m-2 flex flex-col gap-2">
			{/* 官方 Flag Server 地址（比赛期间由 GameBox 内探测/提交，内网可达） */}
			<div className="flex flex-col gap-1 rounded border border-dashed border-gray-300 p-2">
				<p className="font-mono text-xs">
					FlagServer: http://judge-server/flag
				</p>
				<p className="text-xs text-gray-500">
					Available from your GameBox only
				</p>
			</div>
			<AwdpEventWorkbench eventId={id} />
		</div>
	);
}
