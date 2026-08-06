import { UnderlineNav } from "@primer/react";
import { Outlet, createFileRoute, useLocation } from "@tanstack/react-router";

import { RouterNavItem } from "@/routes/service/events/jeopardy.$id/route";

export const Route = createFileRoute("/service/discussions")({
    component: RouteComponent,
});

function RouteComponent() {
    const location = useLocation();
    // Hide tabs on detail pages (/service/discussions/$id)
    const isDetailPage =
        location.pathname !== "/service/discussions" &&
        location.pathname !== "/service/discussions/my";

    return (
        <div>
            {!isDetailPage && (
                <UnderlineNav aria-label="Discussions">
                    <RouterNavItem to="/service/discussions">All</RouterNavItem>
                    <RouterNavItem to="/service/discussions/my">
                        My Discussions
                    </RouterNavItem>
                </UnderlineNav>
            )}
            <Outlet />
        </div>
    );
}
