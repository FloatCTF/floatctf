import { Spinner, UnderlineNav } from "@primer/react";
import { useQuery } from "@tanstack/react-query";
import { Outlet, createFileRoute } from "@tanstack/react-router";
import { createContext } from "react";

import { adminApi } from "@/api";
import {
    type Challenges,
    type EventChallenges,
    EventType,
    type Events,
} from "@/entity";
import { RouterNavItem } from "@/routes/service/events/jeopardy.$id/route";

export const Route = createFileRoute("/admin/events/jeopardy/$id")({
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

    const {
        data: event_data,
        isLoading,
        isError,
    } = useQuery({
        queryKey: ["event", id],
        queryFn: () => adminApi.events.get(id),
    });

    const event = event_data?.data;

    if (isLoading) {
        return <Spinner size="large" />;
    }

    if (isError || !event) {
        return <div>Error loading event</div>;
    }

    return (
        <div>
            <h3>
                {event.title} #{event.id}
            </h3>
            <UnderlineNav aria-label="Repository">
                <RouterNavItem to="/admin/events/jeopardy/$id" params={{ id }}>
                    Challenges
                </RouterNavItem>
                {event?.type === EventType.JeopardySingle && (
                    <RouterNavItem
                        to="/admin/events/jeopardy/$id/users"
                        params={{ id }}
                    >
                        Users
                    </RouterNavItem>
                )}

                {event?.type === EventType.JeopardyTeam && (
                    <RouterNavItem
                        to="/admin/events/jeopardy/$id/teams"
                        params={{ id }}
                    >
                        Teams
                    </RouterNavItem>
                )}
                <RouterNavItem
                    to="/admin/events/jeopardy/$id/announcements"
                    params={{ id }}
                >
                    Announcements
                </RouterNavItem>
                <RouterNavItem
                    to="/admin/events/jeopardy/$id/writeups"
                    params={{ id }}
                >
                    WriteUps
                </RouterNavItem>
                <RouterNavItem
                    to="/admin/events/jeopardy/$id/logs"
                    params={{ id }}
                >
                    Logs
                </RouterNavItem>
                <RouterNavItem
                    to="/admin/events/jeopardy/$id/data_present"
                    params={{ id }}
                >
                    Data Present
                </RouterNavItem>
            </UnderlineNav>
            <EventContext.Provider value={event}>
                <Outlet /> {/* 普通 TanStack Router 的 Outlet */}
            </EventContext.Provider>
        </div>
    );
}
