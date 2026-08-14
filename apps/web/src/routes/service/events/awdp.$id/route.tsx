import { RocketIcon } from "@primer/octicons-react";
import { Spinner, UnderlineNav } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { Outlet, createFileRoute, useMatchRoute } from "@tanstack/react-router";
import { useTitle } from "ahooks";
import { createContext } from "react";

import { serviceApi } from "@/api";
import { AwdpEventProgress } from "@/components/awdp/AwdpEventProgress";
import { useAwdpEventStream } from "@/hooks/useAwdpEventStream";
import { AppLink } from "@/navigation";
import { ServiceRouteGuard } from "../../route";

export const Route = createFileRoute("/service/events/awdp/$id")({
	component: RouteComponent,
	loader: ServiceRouteGuard,
});

export const AwdpEventContext = createContext<{ id: string }>({ id: "" });

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
	// TanStack 路径类型严格；动态 AWDP 子路由需断言。
	const path = to as never;
	const params = { id } as never;
	const isActive = matchRoute({ to: path, params, fuzzy: false });
	return (
		<AppLink style={{ textDecoration: "none" }} to={path} params={params}>
			<UnderlineNav.Item aria-current={isActive ? "page" : undefined}>
				{children}
			</UnderlineNav.Item>
		</AppLink>
	);
}

function RouteComponent() {
	const { id } = Route.useParams();
	const { data, isLoading } = useQuery({
		queryKey: ["eventInfo", id],
		queryFn: () => serviceApi.events.get(id),
	});
	const eventInfo = data?.data;
	useTitle(`${eventInfo?.event.title ?? "AWD Plus Event"} | FloatCTF`);
	// 实时流 + 轮询快照回退。
	const stream = useAwdpEventStream({ eventId: id });

	if (isLoading) {
		return <Spinner size="large" />;
	}

	return (
		<div>
			<div className="flex gap-1 items-center">
				<RocketIcon size={20} />
				<h3 className="font-bold">{eventInfo?.event.title ?? "AWD Plus"}</h3>
				<span className="text-xs opacity-60 ml-2">
					{stream.connected ? "live" : "poll"}
				</span>
			</div>
			{/* 进度条：同 Jeopardy 位置（标题与导航之间），Break/Fix 分段 + 竖线 */}
			<AwdpEventProgress id={id} />
			<AwdpEventContext.Provider value={{ id }}>
				<UnderlineNav aria-label="AWD Plus event">
					{/* 比赛：所有 gamebox 共享同一个顶部进度条，不再单列 Overview / GameBoxes
					   tab（参赛入口与工作台保留在默认落地页）。 */}
					<RouterNavItem to="/service/events/awdp/$id/rounds" id={id}>
						Rounds
					</RouterNavItem>
					<RouterNavItem to="/service/events/awdp/$id/scoreboard" id={id}>
						Scoreboard
					</RouterNavItem>
					<RouterNavItem to="/service/events/awdp/$id/announcement" id={id}>
						Announcement
					</RouterNavItem>
				</UnderlineNav>
				<div className="mt-3">
					<Outlet />
				</div>
			</AwdpEventContext.Provider>
		</div>
	);
}
