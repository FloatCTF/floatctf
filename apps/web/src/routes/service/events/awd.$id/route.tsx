import { RocketIcon } from "@primer/octicons-react";
import { Spinner, UnderlineNav } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import {
	Link,
	Outlet,
	createFileRoute,
	useMatchRoute,
} from "@tanstack/react-router";
import { useTitle } from "ahooks";
import { createContext } from "react";

import { serviceApi } from "@/api";
import { useAwdEventStream } from "@/hooks/useAwdEventStream";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awd/$id")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

export const AwdEventContext = createContext<{ id: string }>({ id: "" });

function RouterNavItem({
	to,
	id,
	children,
}: {
	to: string;
	id: string;
	children: React.ReactNode;
}) {
	const matchRoute = useMatchRoute();
	// TanStack path typing is strict; cast for dynamic AWD sub-routes.
	const path = to as never;
	const params = { id } as never;
	const isActive = matchRoute({ to: path, params, fuzzy: false });
	return (
		<Link style={{ textDecoration: "none" }} to={path} params={params}>
			<UnderlineNav.Item aria-current={isActive ? "page" : undefined}>
				{children}
			</UnderlineNav.Item>
		</Link>
	);
}

function RouteComponent() {
	const { id } = Route.useParams();
	const { data, isLoading } = useQuery({
		queryKey: ["eventInfo", id],
		queryFn: () => serviceApi.events.get(id),
	});
	const eventInfo = data?.data;
	useTitle(`${eventInfo?.event.title ?? "AWD Event"} | FloatCTF`);
	// Realtime + poll snapshot fallback (R7-B stream)
	const stream = useAwdEventStream({ eventId: id });

	if (isLoading) {
		return <Spinner size="large" />;
	}

	return (
		<div>
			<div className="flex gap-1 items-center">
				<RocketIcon size={20} />
				<h3 className="font-bold">{eventInfo?.event.title ?? "AWD"}</h3>
				<span className="text-xs opacity-60 ml-2">
					{stream.connected ? "live" : "poll"}
				</span>
			</div>
			<AwdEventContext.Provider value={{ id }}>
				<UnderlineNav aria-label="AWD event">
					<RouterNavItem to="/service/events/awd/$id" id={id}>
						Overview
					</RouterNavItem>
					<RouterNavItem
						to="/service/events/awd/$id/gameboxes"
						id={id}
					>
						GameBoxes
					</RouterNavItem>
					<RouterNavItem
						to="/service/events/awd/$id/scoreboard"
						id={id}
					>
						Scoreboard
					</RouterNavItem>
					<RouterNavItem
						to="/service/events/awd/$id/wireguard"
						id={id}
					>
						WireGuard
					</RouterNavItem>
				</UnderlineNav>
				<div className="mt-3">
					<Outlet />
				</div>
			</AwdEventContext.Provider>
		</div>
	);
}
