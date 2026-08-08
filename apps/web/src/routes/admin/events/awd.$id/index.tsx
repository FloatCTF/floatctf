import { useNavigate } from "@tanstack/react-router";
import { createFileRoute } from "@tanstack/react-router";
import { useEffect } from "react";

import { AdminRouteGuard } from "../../route";

/**
 * AWD 赛事入口页：GameBox = AWD 的题目模型（§46），没有 Challenges 概念，
 * 默认页直接落在 GameBoxes Tab。
 */
export const Route = createFileRoute("/admin/events/awd/$id/")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const navigate = useNavigate();

	useEffect(() => {
		navigate({
			to: "/admin/events/awd/$id/gameboxes",
			params: { id },
			replace: true,
		});
	}, [id, navigate]);

	return null;
}
