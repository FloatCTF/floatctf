import { Spinner, UnderlineNav } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import {
	Outlet,
	createFileRoute,
	useLocation,
	useNavigate,
} from "@tanstack/react-router";
import { createContext, useEffect } from "react";

import { adminApi } from "@/api";
import {
	type Challenges,
	type EventChallenges,
	EventFamily,
	type Events,
} from "@/entity";
import { RouterNavItem } from "@/routes/service/events/jeopardy.$id/route";

export const Route = createFileRoute("/admin/events/awd/$id")({
	component: RouteComponent,
});

export const EventContext = createContext<Events | null>(null);

export type EventChallengeResult = {
	id: string;
	event_challenge: EventChallenges;
	challenge: Challenges;
};

function RouteComponent() {
	const { id } = Route.useParams();
	const location = useLocation();
	const navigate = useNavigate();
	const eventQuery = useQuery({
		queryKey: ["event", id],
		queryFn: () => adminApi.events.get(id),
	});
	const statusQuery = useQuery({
		queryKey: ["admin-awd-status", id],
		queryFn: () => adminApi.awd.getStatus(id),
	});

	const event = eventQuery.data?.data;
	const configured = Boolean(statusQuery.data?.data);
	useEffect(() => {
		if (
			!statusQuery.isLoading &&
			!statusQuery.isError &&
			!configured &&
			location.pathname !== `/admin/events/awd/${id}/configure`
		) {
			navigate({
				to: "/admin/events/awd/$id/configure",
				params: { id },
				replace: true,
			});
		}
	}, [
		configured,
		id,
		location.pathname,
		navigate,
		statusQuery.isError,
		statusQuery.isLoading,
	]);

	if (eventQuery.isLoading || statusQuery.isLoading) {
		return <Spinner size="large" />;
	}
	if (eventQuery.isError || !event) {
		return <div>Error loading event</div>;
	}
	if (statusQuery.isError) {
		return <div>Error loading AWD configuration</div>;
	}

	return (
		<div>
			<h3>
				{event.title} #{event.id}
			</h3>
			<UnderlineNav aria-label="AWD Event">
				<RouterNavItem to="/admin/events/awd/$id/configure" params={{ id }}>
					Configure
				</RouterNavItem>
				{configured && (
					<>
						<RouterNavItem to="/admin/events/awd/$id/gameboxes" params={{ id }}>
							GameBoxes
						</RouterNavItem>
						<RouterNavItem to="/admin/events/awd/$id/network" params={{ id }}>
							Network
						</RouterNavItem>
						<RouterNavItem to="/admin/events/awd/$id/ops" params={{ id }}>
							Ops
						</RouterNavItem>
						{event.family === EventFamily.Awd && (
							<RouterNavItem to="/admin/events/awd/$id/teams" params={{ id }}>
								Teams
							</RouterNavItem>
						)}
						<RouterNavItem
							to="/admin/events/awd/$id/announcements"
							params={{ id }}
						>
							Announcements
						</RouterNavItem>
						<RouterNavItem to="/admin/events/awd/$id/writeups" params={{ id }}>
							WriteUps
						</RouterNavItem>
						<RouterNavItem to="/admin/events/awd/$id/logs" params={{ id }}>
							Logs
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
