import { useNavigate } from "@tanstack/react-router";
import { createFileRoute } from "@tanstack/react-router";
import { useEffect } from "react";

import { AdminRouteGuard } from "../../route";

/**
 * AWDP 赛事入口页：Configure 是 AWDP 参数与首次保存的唯一入口。
 */
export const Route = createFileRoute("/admin/events/awdp/$id/")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	const navigate = useNavigate();

	useEffect(() => {
		navigate({
			to: "/admin/events/awdp/$id/configure",
			params: { id },
			replace: true,
		});
	}, [id, navigate]);

	return null;
}
