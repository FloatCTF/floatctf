import { Spinner, UnderlineNav } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { Outlet, createFileRoute } from "@tanstack/react-router";
import { createContext } from "react";

import { adminApi } from "@/api";
import { awdpAdminApi } from "@/api/awdp";
import type { Events } from "@/entity";
import { RouterNavItem } from "@/routes/service/events/jeopardy.$id/route";

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
	const configQuery = useQuery({
		queryKey: ["awdp-config", id],
		queryFn: () => awdpAdminApi.getConfig(id),
		retry: false,
	});

	const event = eventQuery.data?.data;
	const configured = Boolean(configQuery.data?.data);

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
			<UnderlineNav aria-label="AWDP Event">
				{event.is_virtual ? (
					<>
						<RouterNavItem to="/admin/events/awdp/$id/instance" params={{ id }}>
							Instance
						</RouterNavItem>
						<RouterNavItem to="/admin/events/awdp/$id/logs" params={{ id }}>
							Logs
						</RouterNavItem>
						<RouterNavItem to="/admin/events/awdp/$id/judge" params={{ id }}>
							Judge
						</RouterNavItem>
					</>
				) : (
					<>
						<RouterNavItem to="/admin/events/awdp/$id/configure" params={{ id }}>
							Configure
						</RouterNavItem>
						{configured && (
							<>
								<RouterNavItem to="/admin/events/awdp/$id/gameboxes" params={{ id }}>
									GameBoxes
								</RouterNavItem>
								<RouterNavItem to="/admin/events/awdp/$id/ops" params={{ id }}>
									Ops
								</RouterNavItem>
							</>
						)}
						<RouterNavItem to="/admin/events/awdp/$id/instance" params={{ id }}>
							Instance
						</RouterNavItem>
					</>
				)}
			</UnderlineNav>
			<EventContext.Provider value={event}>
				<Outlet />
			</EventContext.Provider>
		</div>
	);
}
