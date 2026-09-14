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
	PackageIcon,
	PasskeyFillIcon,
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
 * 管理端信息架构（GitHub Settings / Primer 风格）。
 *
 * - Section 为静态标签；group 可折叠；leaf 负责导航。
 * - 赛事详情页与 Docker 标签留在页内导航；
 *   侧栏只表达稳定的全局信息架构。
 * - 「全部赛事」通过显式模式匹配列表与 Jeopardy/AWD 详情
 *   （禁止笼统 startsWith），未来 `/admin/events/*` 页面
 *   须先做 IA 决策再归类。
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
		id: "events",
		label: "Events",
		children: [
			item("events.all", "All Events", "/admin/events", {
				icon: <CalendarIcon />,
				match: {
					mode: "pattern",
					pattern: /^\/admin\/events(?:\/(?:jeopardy|awd|awdp)\/[^/]+(?:\/.*)?)?$/,
				},
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
			item("content.gameboxes", "GameBoxes", "/admin/awd/gameboxes", {
				icon: <PackageIcon />,
			}),
			item("content.weapons", "Weapons", "/admin/weapons", {
				icon: <GiftIcon />,
			}),
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
			item(
				"infrastructure.networking",
				"AWD Networking",
				"/admin/awd/network",
				{ icon: <KeyIcon /> },
			),
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
