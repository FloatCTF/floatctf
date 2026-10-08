/**
 * 导航配置 —— 信息架构的唯一出处（侧边栏、移动端导航、命令面板、面包屑都从这里派生）。
 * 路由路径由本前端自主决定，与 Default Frontend 无关。
 */

import {
	Activity,
	Blocks,
	BookOpen,
	Boxes,
	CalendarDays,
	Container,
	Database,
	Dumbbell,
	Gauge,
	Home,
	Info,
	ListChecks,
	Megaphone,
	MessagesSquare,
	Network,
	PanelsTopLeft,
	Puzzle,
	ScrollText,
	Settings,
	Shield,
	Swords,
	Terminal,
	Timer,
	Trophy,
	UserCircle,
	UserCog,
	Users,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";

export interface NavItem {
	to: string;
	label: string;
	icon: LucideIcon;
	/** 精确匹配（用于 `/` 这类根路径）。 */
	end?: boolean;
	description?: string;
}

export interface NavGroup {
	label: string;
	items: NavItem[];
}

export const playerNav: NavGroup[] = [
	{
		label: "Play",
		items: [
			{ to: "/", label: "总览", icon: Home, end: true, description: "我的比赛与待办" },
			{ to: "/events", label: "赛事", icon: CalendarDays, description: "全部赛事与工作区" },
			{ to: "/challenges", label: "题库", icon: Puzzle, description: "练习题库" },
			{ to: "/challenge-sets", label: "题集", icon: Blocks, description: "按主题组织的题目集合" },
			{ to: "/training", label: "训练场", icon: Dumbbell, description: "AWDP 练习靶机" },
			{ to: "/instances", label: "我的实例", icon: Boxes, description: "已启动的题目环境" },
		],
	},
	{
		label: "Community",
		items: [
			{ to: "/rank", label: "排行榜", icon: Trophy, description: "Top15 选手" },
			{ to: "/solves", label: "解题流水", icon: Activity, description: "我的练习解题记录" },
			{ to: "/community/announcements", label: "公告", icon: Megaphone },
			{ to: "/community/discussions", label: "讨论区", icon: MessagesSquare },
			{ to: "/writeups", label: "题解", icon: BookOpen },
			{ to: "/arsenal", label: "武器库", icon: Swords },
		],
	},
	{
		label: "Me",
		items: [{ to: "/profile", label: "我的资料", icon: UserCircle }],
	},
];

export const adminNav: NavGroup[] = [
	{
		label: "控制台",
		items: [
			{ to: "/admin", label: "总览", icon: Gauge, end: true, description: "平台聚合状态" },
			{ to: "/admin/events", label: "赛事", icon: CalendarDays, description: "赛事控制台" },
		],
	},
	{
		label: "内容",
		items: [
			{ to: "/admin/challenges", label: "题库", icon: Puzzle, description: "题目导入 / 构建 / 校验" },
			{ to: "/admin/challenge-sets", label: "题集", icon: Blocks },
			{ to: "/admin/gameboxes", label: "GameBox 库", icon: Boxes },
			{ to: "/admin/network", label: "靶场网络", icon: Network },
		],
	},
	{
		label: "社区",
		items: [
			{ to: "/admin/community/announcements", label: "公告", icon: Megaphone },
			{ to: "/admin/community/discussions", label: "讨论", icon: MessagesSquare },
			{ to: "/admin/community/weapons", label: "武器库", icon: Swords },
		],
	},
	{
		label: "平台",
		items: [
			{ to: "/admin/platform/users", label: "用户", icon: Users },
			{ to: "/admin/platform/super-admins", label: "超管", icon: UserCog },
			{ to: "/admin/platform/logs", label: "日志", icon: ScrollText },
			{ to: "/admin/platform/settings", label: "设置", icon: Settings },
			{ to: "/admin/platform/frontends", label: "前端", icon: PanelsTopLeft },
			{ to: "/admin/platform/tasks", label: "计划任务", icon: Timer },
		],
	},
	{
		label: "基础设施",
		items: [
			{ to: "/admin/infra/docker", label: "Docker", icon: Container },
			{ to: "/admin/infra/sql", label: "SQL 控制台", icon: Database },
			{ to: "/admin/infra/terminal", label: "终端", icon: Terminal },
			{ to: "/admin/version", label: "版本", icon: Info },
		],
	},
];

/** 移动端底部导航（只放最高频入口）。 */
export const playerMobileNav: NavItem[] = [
	{ to: "/", label: "总览", icon: Home, end: true },
	{ to: "/events", label: "赛事", icon: CalendarDays },
	{ to: "/challenges", label: "题库", icon: Puzzle },
	{ to: "/rank", label: "排行", icon: Trophy },
	{ to: "/profile", label: "我的", icon: UserCircle },
];

export const adminMobileNav: NavItem[] = [
	{ to: "/admin", label: "总览", icon: Gauge, end: true },
	{ to: "/admin/events", label: "赛事", icon: CalendarDays },
	{ to: "/admin/challenges", label: "题库", icon: Puzzle },
	{ to: "/admin/platform/users", label: "用户", icon: Users },
];

export function flatNav(groups: NavGroup[]): NavItem[] {
	return groups.flatMap((group) => group.items);
}

/** 事件日志 / 列表里常用的安全图标。 */
export const navIcons = { Shield, ListChecks };
