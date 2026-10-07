import { useNavigate } from "@tanstack/react-router";
import { createFileRoute } from "@tanstack/react-router";
import { useContext, useEffect } from "react";

import { AdminRouteGuard } from "../../route";
import { EventContext } from "./route";

/**
 * AWDP 赛事入口页：普通赛事默认 Configure；虚拟（训练）赛事只有
 * Instance / Logs 两个 tab，默认进 Instance。
 */
export const Route = createFileRoute("/admin/events/awdp/$id/")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const event = useContext(EventContext);
	const navigate = useNavigate();

	useEffect(() => {
		navigate({
			to: event?.is_virtual
				? "/admin/events/awdp/$id/instance"
				: "/admin/events/awdp/$id/configure",
			params: { id },
			replace: true,
		});
	}, [event?.is_virtual, id, navigate]);

	return null;
}
