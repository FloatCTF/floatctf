import { Spinner } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { Outlet, createFileRoute } from "@tanstack/react-router";
import { createContext } from "react";

import { adminApi } from "@/api";
import type { Events } from "@/entity";

export const Route = createFileRoute("/admin/events/awdp/$id")({
	component: RouteComponent,
});

export const EventContext = createContext<Events | null>(null);

function RouteComponent() {
	const { id } = Route.useParams();
	const eventQuery = useQuery({
		queryKey: ["event", id],
		queryFn: () => adminApi.events.get(id),
	});

	const event = eventQuery.data?.data;

	if (eventQuery.isLoading) {
		return <Spinner size="large" />;
	}
	if (eventQuery.isError || !event) {
		return <div>Error loading event</div>;
	}

	return (
		<div>
			<h3>
				{event.title} #{event.id}
			</h3>
			<EventContext.Provider value={event}>
				<Outlet />
			</EventContext.Provider>
		</div>
	);
}
