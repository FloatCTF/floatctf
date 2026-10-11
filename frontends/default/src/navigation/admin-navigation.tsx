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

import type {
	NavigationItem,
	NavigationSection,
	NavigationVisibilityContext,
} from "./sidebar-types";

const item = (
	id: string,
	label: string,
	href: string,
	options: {
		icon?: React.ReactNode;
		match?: NavigationItem["match"];
		/**
		 * 该入口所需权限（v2）。仅用于**界面显隐**，不是授权依据 ——
		 * 真正的判定在服务端每个请求上（`RequirePermission<P>`）。
		 * 若只靠隐藏入口来"保护"页面，直接输入 URL 就能绕过。
		 */
		permission?: string;
	} = {},
): NavigationItem => ({
	type: "item",
	id,
	label,
	href,
	icon: options.icon,
	match: options.match ?? { mode: "exact" },
	...(options.permission
		? {
				isVisible: ({ permissions }: NavigationVisibilityContext) =>
					permissions.has(options.permission as string),
			}
		: {}),
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
				permission: "system.read",
				icon: <GoalIcon />,
				match: { mode: "exact" },
			}),
		],
	},
	{
		id: "access",
		label: "Access",
		children: [
			// v2：不再有「Super Admins」入口 —— 管理员不是独立账号类型，
			// 而是持有 super-admin 角色的账号（见 Users / Roles）。
			// Wave 10 已随 super_admin 表一起删除该页面与它的 legacy 端点。
			item("access.users", "Users", "/admin/users", {
				permission: "users.read",
				icon: <PasskeyFillIcon />,
			}),
			item("access.roles", "Roles", "/admin/roles", {
				permission: "roles.read",
				icon: <ShieldLockIcon />,
			}),
		],
	},
	{
		id: "events",
		label: "Events",
		children: [
			item("events.all", "All Events", "/admin/events", {
				permission: "events.read",
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
							permission: "challenges.read",
							icon: <ListUnorderedIcon />,
						},
					),
					item(
						"content.challenges.sets",
						"Challenge Sets",
						"/admin/challenge_sets",
						{
							permission: "challenges.read",
							match: { mode: "segment-prefix" },
						},
					),
				],
			},
			item("content.gameboxes", "GameBoxes", "/admin/awd/gameboxes", {
				permission: "gameboxes.read",
				icon: <PackageIcon />,
			}),
			item("content.weapons", "Weapons", "/admin/weapons", {
				permission: "weapons.read",
				icon: <GiftIcon />,
			}),
		],
	},
	{
		id: "community",
		label: "Community",
		children: [
			item("community.announcements", "Announcements", "/admin/announcements", {
				permission: "announcements.manage",
				icon: <MegaphoneIcon />,
			}),
			item("community.discussions", "Discussions", "/admin/discussions", {
				permission: "discussions.moderate",
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
				{
					permission: "awd.read",
					icon: <KeyIcon />,
				},
			),
			item("infrastructure.terminal", "Terminal", "/admin/terminal", {
				permission: "terminal.open",
				icon: <TerminalIcon />,
			}),
			item("infrastructure.docker", "Docker", "/admin/docker", {
				permission: "docker.read",
				icon: <ContainerIcon />,
				match: { mode: "segment-prefix" },
			}),
			item("infrastructure.database", "Database", "/admin/database", {
				permission: "database.execute",
				icon: <DatabaseIcon />,
			}),
		],
	},
	{
		id: "operations",
		label: "Operations",
		children: [
			item("operations.logs", "Logs", "/admin/logs", {
				permission: "logs.read",
				icon: <LogIcon />,
			}),
			item(
				"operations.scheduled-tasks",
				"Scheduled Tasks",
				"/admin/scheduled_tasks",
				{
					permission: "scheduler.manage",
					icon: <ClockIcon />,
				},
			),
		],
	},
	{
		id: "system",
		label: "System",
		children: [
			item("system.settings", "Settings", "/admin/settings", {
				permission: "settings.read",
				icon: <GearIcon />,
			}),
			item("system.version", "Version", "/admin/version", {
				permission: "system.read",
				icon: <ZapIcon />,
			}),
		],
	},
];

export const adminIgnoreRoutes: readonly string[] = ["/admin", "/admin/"];
