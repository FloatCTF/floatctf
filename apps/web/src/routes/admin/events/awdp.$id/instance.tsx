import { createFileRoute } from "@tanstack/react-router";

import { EventInstancesTable } from "@/components/admin/EventInstancesTable";
import { AdminRouteGuard } from "@/routes/admin/route";

export const Route = createFileRoute("/admin/events/awdp/$id/instance")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	return <EventInstancesTable eventId={id} />;
}
