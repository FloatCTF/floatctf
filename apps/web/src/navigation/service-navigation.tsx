import {
	CommentDiscussionIcon,
	FlameIcon,
	GiftIcon,
	GoalIcon,
	ListUnorderedIcon,
	LogIcon,
	MegaphoneIcon,
	NoteIcon,
	PersonIcon,
	TasklistIcon,
	TelescopeIcon,
} from "@primer/octicons-react";

import type { NavigationItem, NavigationSection } from "./sidebar-types";

const item = (
	id: string,
	label: string,
	href: string,
	options: {
		icon?: React.ReactNode;
		match?: NavigationItem["match"];
	} = {},
): NavigationItem => ({
	type: "item",
	id,
	label,
	href,
	icon: options.icon,
	match: options.match ?? { mode: "exact" },
});

/**
 * Player (service) information architecture (GitHub Settings / Primer-style).
 *
 * - Top Users is the service landing (`/service` redirects there), kept in a
 *   label-less section like the admin Dashboard.
 * - Events/Challenges/Challenge Sets/Discussions/Writeups use segment-prefix
 *   matching so their detail routes activate the right leaf; the sidebar is
 *   hidden on event-detail pages by the service layout, but the leaf still
 *   behaves correctly on the list pages.
 * - segment boundaries keep `/service/challenges` from matching
 *   `/service/challenge_sets` and vice versa.
 */
export const serviceNavigation: NavigationSection[] = [
	{
		id: "overview",
		children: [
			item("service.top", "Top Users", "/service/top", {
				icon: <GoalIcon />,
			}),
		],
	},
	{
		id: "competition",
		label: "Competition",
		children: [
			item("service.events", "Events", "/service/events", {
				icon: <TelescopeIcon />,
				match: { mode: "segment-prefix" },
			}),
			item("service.challenges", "Challenges", "/service/challenges", {
				icon: <ListUnorderedIcon />,
				match: { mode: "segment-prefix" },
			}),
			item(
				"service.challenge-sets",
				"Challenge Sets",
				"/service/challenge_sets",
				{
					icon: <TasklistIcon />,
					match: { mode: "segment-prefix" },
				},
			),
			item("service.solves", "Solves", "/service/solves", {
				icon: <LogIcon />,
			}),
			item("service.instances", "Instances", "/service/instances", {
				icon: <FlameIcon />,
			}),
		],
	},
	{
		id: "community",
		label: "Community",
		children: [
			item("service.announcements", "Announcements", "/service/announcements", {
				icon: <MegaphoneIcon />,
			}),
			item("service.discussions", "Discussions", "/service/discussions", {
				icon: <CommentDiscussionIcon />,
				match: { mode: "segment-prefix" },
			}),
			item("service.writeups", "Writeups", "/service/writeups", {
				icon: <NoteIcon />,
				match: { mode: "segment-prefix" },
			}),
		],
	},
	{
		id: "arsenal",
		label: "Arsenal",
		children: [
			item("service.weapons", "Weapons", "/service/weapons", {
				icon: <GiftIcon />,
			}),
		],
	},
	{
		id: "account",
		label: "Account",
		children: [
			item("service.profile", "Profile", "/service/profile", {
				icon: <PersonIcon />,
			}),
		],
	},
];
