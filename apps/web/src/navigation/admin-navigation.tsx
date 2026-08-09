import {
	CalendarIcon,
	ClockIcon,
	CommentDiscussionIcon,
	ContainerIcon,
	DatabaseIcon,
	GearIcon,
	GiftIcon,
	GoalIcon,
	KeyIcon,
	ListUnorderedIcon,
	LogIcon,
	MegaphoneIcon,
	PasskeyFillIcon,
	ServerIcon,
	ShieldLockIcon,
	TasklistIcon,
	TerminalIcon,
	TrophyIcon,
	ZapIcon,
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
 * Admin information architecture (GitHub Settings / Primer-style).
 *
 * - Sections are static labels; groups toggle; leaves navigate.
 * - Event detail pages and Docker tabs stay inside page-level navigation;
 *   the sidebar only expresses stable global IA.
 * - All Events matches the list plus Jeopardy/AWD detail routes via an
 *   explicit pattern (never a blanket startsWith) so future
 *   `/admin/events/*` pages require an IA decision to be categorized.
 */
export const adminNavigation: NavigationSection[] = [
	{
		id: "overview",
		children: [
			item("dashboard", "Dashboard", "/admin/dashboard", {
				icon: <GoalIcon />,
				match: { mode: "exact" },
			}),
		],
	},
	{
		id: "access",
		label: "Access",
		children: [
			item("access.super-admins", "Super Admins", "/admin/super_admins", {
				icon: <ShieldLockIcon />,
			}),
			item("access.users", "Users", "/admin/users", {
				icon: <PasskeyFillIcon />,
			}),
		],
	},
	{
		id: "content",
		label: "Content",
		children: [
			{
				type: "group",
				id: "content.challenges",
				label: "Challenges",
				icon: <TrophyIcon />,
				children: [
					item(
						"content.challenges.all",
						"All Challenges",
						"/admin/challenges",
						{
							icon: <ListUnorderedIcon />,
						},
					),
					item(
						"content.challenges.sets",
						"Challenge Sets",
						"/admin/challenge_sets",
						{ match: { mode: "segment-prefix" } },
					),
				],
			},
			item("content.weapons", "Weapons", "/admin/weapons", {
				icon: <GiftIcon />,
			}),
		],
	},
	{
		id: "competition",
		label: "Competition",
		children: [
			{
				type: "group",
				id: "competition.events",
				label: "Events",
				icon: <CalendarIcon />,
				children: [
					item("competition.events.all", "All Events", "/admin/events", {
						match: {
							mode: "pattern",
							pattern:
								/^\/admin\/events(?:\/(?:jeopardy|awd)\/[^/]+(?:\/.*)?)?$/,
						},
					}),
					{
						type: "group",
						id: "competition.events.awd",
						label: "AWD",
						children: [
							item(
								"competition.events.awd.gameboxes",
								"GameBoxes",
								"/admin/awd/gameboxes",
							),
							item(
								"competition.events.awd.networking",
								"Networking",
								"/admin/awd/network",
								{ icon: <KeyIcon /> },
							),
						],
					},
				],
			},
		],
	},
	{
		id: "community",
		label: "Community",
		children: [
			item("community.announcements", "Announcements", "/admin/announcements", {
				icon: <MegaphoneIcon />,
			}),
			item("community.discussions", "Discussions", "/admin/discussions", {
				icon: <CommentDiscussionIcon />,
			}),
		],
	},
	{
		id: "infrastructure",
		label: "Infrastructure",
		children: [
			item("infrastructure.instances", "Instances", "/admin/instances", {
				icon: <ServerIcon />,
			}),
			item("infrastructure.terminal", "Terminal", "/admin/terminal", {
				icon: <TerminalIcon />,
			}),
			item("infrastructure.docker", "Docker", "/admin/docker", {
				icon: <ContainerIcon />,
				match: { mode: "segment-prefix" },
			}),
			item("infrastructure.database", "Database", "/admin/database", {
				icon: <DatabaseIcon />,
			}),
		],
	},
	{
		id: "operations",
		label: "Operations",
		children: [
			item("operations.logs", "Logs", "/admin/logs", {
				icon: <LogIcon />,
			}),
			item(
				"operations.scheduled-tasks",
				"Scheduled Tasks",
				"/admin/scheduled_tasks",
				{ icon: <ClockIcon /> },
			),
		],
	},
	{
		id: "system",
		label: "System",
		children: [
			item("system.settings", "Settings", "/admin/settings", {
				icon: <GearIcon />,
			}),
			item("system.version", "Version", "/admin/version", {
				icon: <ZapIcon />,
			}),
		],
	},
];

export const adminIgnoreRoutes: readonly string[] = ["/admin", "/admin/"];
