import {
	CommentDiscussionIcon,
	FlameIcon,
	GiftIcon,
	GoalIcon,
	ListUnorderedIcon,
	LogIcon,
	MegaphoneIcon,
	NoteIcon,
	PackageIcon,
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
 * 选手端（service）信息架构（GitHub Settings / Primer 风格）。
 *
 * - Top Users 为选手端落地页（`/service` 重定向至此），放在无标签分区，
 *   与管理端 Dashboard 类似。
 * - Events/Challenges/Challenge Sets/Discussions/Writeups 用路径段前缀匹配，
 *   使详情路由高亮正确叶子；赛事详情页由 service layout 隐藏侧栏，
 *   但列表页叶子行为仍正确。
 * - 段边界保证 `/service/challenges` 不会误匹配
 *   `/service/challenge_sets`，反之亦然。
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
			item("service.gameboxes", "Gameboxes", "/service/gameboxes", {
				icon: <PackageIcon />,
				match: { mode: "segment-prefix" },
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
