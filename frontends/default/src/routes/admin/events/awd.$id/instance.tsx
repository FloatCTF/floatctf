import { createFileRoute } from "@tanstack/react-router";

import { EventInstancesTable } from "@/components/admin/EventInstancesTable";
import { AdminRouteGuard } from "@/routes/admin/route";

export const Route = createFileRoute("/admin/events/awd/$id/instance")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

function RouteComponent() {
	const { id } = Route.useParams();
	return <EventInstancesTable eventId={id} />;
}
