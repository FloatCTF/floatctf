import type { FeatureModule } from "~/features/types";

import { EventChallengePage, EventsListPage, EventWorkspacePage } from "./pages";

export const feature: FeatureModule = {
	shell: "player",
	routes: [
		{ path: "/events", element: <EventsListPage /> },
		{ path: "/events/:eventId", element: <EventWorkspacePage /> },
		{ path: "/events/:eventId/challenges/:challengeId", element: <EventChallengePage /> },
	],
};
