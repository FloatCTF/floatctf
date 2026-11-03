/**
 * 信息架构 —— 导航的唯一出处（侧栏、命令面板、面包屑都从这里派生）。
 *
 * 设计取舍：
 *
 * - **两个工作区**（岛屿 / 控制台）而不是两套互不相干的导航：同一个账号
 *   可能既能参赛也能管理，用顶栏切换比用两条导航树更贴近真实身份。
 * - 每个控制台入口都声明**所需权限**（`permission`），`null` 表示"只要登录"。
 *   这只是**界面显隐** —— 真正的判定在服务端每个请求上。
 *   漏声明一项，它就会显示给任何能进控制台的账号，点进去才吃 403。
 * - 类型上把 `permission` 设为**必填**（`AdminNavItem`）：本仓库没有测试运行器，
 *   门禁是 `tsc --noEmit` + `vite build`，因此让"忘记声明"变成**编译错误**
 *   比写一个可能被删掉的测试更可靠。
 */
import type { IconComponent } from "naive-icons";
import {
	BellIcon,
	BookIcon,
	CalendarIcon,
	ChatIcon,
	ClockIcon,
	CompassIcon,
	FolderIcon,
	FlagIcon,
	GiftIcon,
	GlobeIcon,
	GridIcon,
	HomeIcon,
	InboxIcon,
	InfoIcon,
	KeyIcon,
	ListIcon,
	MagnetIcon,
	MonitorIcon,
	SettingsIcon,
	ShieldIcon,
	TrophyIcon,
	UserIcon,
	UsersIcon,
} from "naive-icons";

export interface NavItem {
	to: string;
	label: string;
	icon: IconComponent;
	/** 精确匹配（工作区首页）。 */
	end?: boolean;
	/** 需要的前端可见权限；null = 只要登录即可。 */
	permission: string | null;
	/** 仅在该平台能力存在时显示（对应 context.capabilities）。 */
	requiresCapability?: string;
}

export interface NavGroup {
	label: string;
	items: NavItem[];
}

/** 选手端（岛屿）。全部只要登录。 */
export const ISLAND_NAV: NavGroup[] = [
	{
		label: "岛屿",
		items: [
			{ to: "/island", label: "总览", icon: HomeIcon, end: true, permission: null },
			{ to: "/island/events", label: "赛事", icon: FlagIcon, permission: null },
			{ to: "/island/challenges", label: "题库", icon: MagnetIcon, permission: null },
			{ to: "/island/scoreboard", label: "排行", icon: TrophyIcon, permission: null },
		],
	},
	{
		label: "社区",
		items: [
			{ to: "/island/discussions", label: "讨论", icon: ChatIcon, permission: null },
			{ to: "/island/writeups", label: "题解", icon: BookIcon, permission: null },
			{ to: "/island/awdp/practice", label: "AWDP 练习", icon: ShieldIcon, permission: null },
		],
	},
	{
		label: "我",
		items: [{ to: "/island/profile", label: "我的资料", icon: UserIcon, permission: null }],
	},
];

/**
 * 管理端（控制台）。`permission` **必填** —— 见文件头说明。
 */
export interface AdminNavItem extends NavItem {
	permission: string;
}

export interface AdminNavGroup {
	label: string;
	items: AdminNavItem[];
}

export const CONSOLE_NAV: AdminNavGroup[] = [
	{
		label: "总览",
		items: [
			{ to: "/console", label: "控制台", icon: CompassIcon, end: true, permission: "system.read" },
		],
	},
	{
		label: "赛事",
		items: [
			{ to: "/console/events", label: "赛事管理", icon: CalendarIcon, permission: "events.read" },
			{ to: "/console/challenges", label: "题库管理", icon: MagnetIcon, permission: "challenges.read" },
			{ to: "/console/gameboxes", label: "箱子库", icon: GiftIcon, permission: "gameboxes.read" },
			{ to: "/console/weapons", label: "武器库", icon: ShieldIcon, permission: "weapons.read" },
			{ to: "/console/network", label: "靶场网络", icon: GlobeIcon, permission: "awd.read" },
		],
	},
	{
		label: "社区",
		items: [
			{ to: "/console/announcements", label: "公告", icon: BellIcon, permission: "announcements.manage" },
			{ to: "/console/discussions", label: "讨论治理", icon: ChatIcon, permission: "discussions.moderate" },
		],
	},
	{
		label: "平台",
		items: [
			{ to: "/console/users", label: "用户", icon: UsersIcon, permission: "users.read" },
			{ to: "/console/roles", label: "角色与权限", icon: KeyIcon, permission: "roles.read" },
			{ to: "/console/logs", label: "审计日志", icon: ListIcon, permission: "logs.read" },
			{ to: "/console/settings", label: "动态设置", icon: SettingsIcon, permission: "settings.read" },
			{ to: "/console/frontends", label: "前端管理", icon: GridIcon, permission: "settings.manage" },
			{ to: "/console/tasks", label: "计划任务", icon: ClockIcon, permission: "scheduler.manage" },
		],
	},
	{
		label: "基础设施",
		items: [
			{ to: "/console/docker", label: "容器运维", icon: InboxIcon, permission: "docker.read" },
			{ to: "/console/database", label: "SQL 控制台", icon: FolderIcon, permission: "database.execute" },
			{ to: "/console/terminal", label: "Web 终端", icon: MonitorIcon, permission: "terminal.open" },
			{ to: "/console/version", label: "版本", icon: InfoIcon, permission: "system.read" },
		],
	},
];

/** 命令面板用的扁平索引（两个工作区一起）。 */
export function flatNav(
	permissionSet: ReadonlySet<string>,
): Array<{ to: string; label: string; group: string; icon: IconComponent }> {
	const out: Array<{ to: string; label: string; group: string; icon: IconComponent }> = [];
	for (const g of ISLAND_NAV) {
		for (const i of g.items) out.push({ to: i.to, label: i.label, group: `岛屿 · ${g.label}`, icon: i.icon });
	}
	for (const g of CONSOLE_NAV) {
		for (const i of g.items) {
			if (!permissionSet.has(i.permission)) continue;
			out.push({ to: i.to, label: i.label, group: `控制台 · ${g.label}`, icon: i.icon });
		}
	}
	return out;
}

/** 按权限裁剪控制台导航；空分组整组隐藏（不留空标题）。 */
export function visibleConsoleNav(permissionSet: ReadonlySet<string>): AdminNavGroup[] {
	return CONSOLE_NAV.map((g) => ({
		...g,
		items: g.items.filter((i) => permissionSet.has(i.permission)),
	})).filter((g) => g.items.length > 0);
}
