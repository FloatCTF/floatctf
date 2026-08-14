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
		<div className="m-2">
			<AwdpEventWorkbench eventId={id} />
		</div>
	);
}
