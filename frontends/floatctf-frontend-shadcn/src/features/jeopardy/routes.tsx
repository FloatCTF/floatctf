import type { FeatureModule } from "~/features/types";

import {
	ChallengeDetailPage,
	ChallengeSetDetailPage,
	ChallengeSetsPage,
	ChallengesCatalogPage,
} from "./pages";

export const feature: FeatureModule = {
	shell: "player",
	routes: [
		{ path: "/challenges", element: <ChallengesCatalogPage /> },
		{ path: "/challenges/:challengeId", element: <ChallengeDetailPage /> },
		{ path: "/challenge-sets", element: <ChallengeSetsPage /> },
		{ path: "/challenge-sets/:setId", element: <ChallengeSetDetailPage /> },
	],
};
