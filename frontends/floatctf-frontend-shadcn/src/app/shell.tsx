/**
 * 应用外壳（选手端 Field / 管理端 Control Room 共用一套实现，靠导航配置区分）。
 *
 * 交互模型：左侧可折叠导航（图标态保留）→ 顶栏（命令面板 / 主题 / 账号）→ 主区；
 * 移动端侧边栏收进 Sheet，另有底部导航。主区永远由 `<Outlet/>` 渲染当前页面。
 */

import type { ReactNode } from "react";
import { NavLink, Outlet, useLocation, useNavigate } from "react-router";
import { ChevronsUpDown, LogOut, Monitor, Moon, Sun, UserCircle } from "lucide-react";

import { useAuthStore, useMe } from "~/auth/store";
import { CommandTrigger } from "~/app/command-palette";
import { UserAvatar } from "~/components/app/user-cell";
import { Button } from "~/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "~/components/ui/dropdown-menu";
import {
	Sidebar,
	SidebarContent,
	SidebarFooter,
	SidebarGroup,
	SidebarGroupContent,
	SidebarGroupLabel,
	SidebarHeader,
	SidebarInset,
	SidebarMenu,
	SidebarMenuButton,
	SidebarMenuItem,
	SidebarProvider,
	SidebarRail,
	SidebarTrigger,
} from "~/components/ui/sidebar";
import { setThemePreference, useTheme } from "~/lib/theme";
import { cn } from "~/lib/utils";

import {
	adminNav as adminNavGroups,
	adminMobileNav,
	type NavGroup,
	type NavItem,
	playerNav as playerNavGroups,
	playerMobileNav,
} from "./nav";

export interface AppShellProps {
	/** 品牌标题 / 副标题。 */
	brand: string;
	brandSubtitle: string;
	navGroups: NavGroup[];
	mobileNav: NavItem[];
	/** 账号作用域：决定登出清哪个 token、在哪看资料。 */
	scope: "user" | "admin";
	children?: ReactNode;
}

function isActivePath(pathname: string, item: NavItem): boolean {
	if (item.end) return pathname === item.to;
	return pathname === item.to || pathname.startsWith(`${item.to}/`);
}

function currentTitle(groups: NavGroup[], pathname: string): string {
	const items = groups.flatMap((group) => group.items);
	// 取最长匹配（最具体的路径优先）。
	const match = items
		.filter((item) => isActivePath(pathname, item))
		.sort((a, b) => b.to.length - a.to.length)[0];
	return match?.label ?? "";
}

export function AppShell({ brand, brandSubtitle, navGroups, mobileNav, scope }: AppShellProps) {
	const location = useLocation();
	const navigate = useNavigate();
	const theme = useTheme();
	const me = useMe();
	const clearScope = useAuthStore((state) => state.clearScope);
	const clearAll = useAuthStore((state) => state.clearAll);
	const adminToken = useAuthStore((state) => state.adminToken);

	const title = currentTitle(navGroups, location.pathname);
	const identity = scope === "admin" ? "管理端会话" : (me?.nickname ?? me?.username ?? "选手会话");

	function logout() {
		if (scope === "admin") clearScope("admin");
		else clearAll();
		void navigate(scope === "admin" ? "/admin/login" : "/login", { replace: true });
	}

	return (
		<SidebarProvider>
			<Sidebar collapsible="icon">
				<SidebarHeader>
					<div className="flex items-center gap-2 px-1 py-1.5">
						<div className="flex size-8 shrink-0 items-center justify-center rounded-md bg-primary text-xs font-semibold text-primary-foreground">
							{brand.slice(0, 1)}
						</div>
						<div className="grid min-w-0 flex-1 text-left leading-tight group-data-[collapsible=icon]:hidden">
							<span className="truncate text-sm font-semibold">{brand}</span>
							<span className="truncate text-xs text-muted-foreground">{brandSubtitle}</span>
						</div>
					</div>
				</SidebarHeader>

				<SidebarContent>
					{navGroups.map((group) => (
						<SidebarGroup key={group.label}>
							<SidebarGroupLabel>{group.label}</SidebarGroupLabel>
							<SidebarGroupContent>
								<SidebarMenu>
									{group.items.map((item) => {
										const Icon = item.icon;
										return (
											<SidebarMenuItem key={item.to}>
												<SidebarMenuButton
													asChild
													isActive={isActivePath(location.pathname, item)}
													tooltip={item.label}
												>
													<NavLink to={item.to} end={item.end}>
														<Icon />
														<span>{item.label}</span>
													</NavLink>
												</SidebarMenuButton>
											</SidebarMenuItem>
										);
									})}
								</SidebarMenu>
							</SidebarGroupContent>
						</SidebarGroup>
					))}
				</SidebarContent>

				<SidebarFooter>
					<div className="flex items-center gap-2 px-1 py-1.5 group-data-[collapsible=icon]:hidden">
						<span className="truncate text-xs text-muted-foreground">{identity}</span>
					</div>
				</SidebarFooter>
				<SidebarRail />
			</Sidebar>

			<SidebarInset className="min-w-0">
				<header className="sticky top-0 z-20 flex h-14 shrink-0 items-center gap-2 border-b bg-background/80 px-3 backdrop-blur lg:px-4">
					<SidebarTrigger aria-label="切换导航" />
					<div className="min-w-0 flex-1">
						<p className="truncate text-sm font-medium">{title}</p>
					</div>

					<CommandTrigger className="hidden h-9 items-center justify-between gap-2 rounded-md border bg-muted/40 px-2.5 text-sm text-muted-foreground hover:bg-muted md:inline-flex md:w-56" />

					<DropdownMenu>
						<DropdownMenuTrigger asChild>
							<Button variant="ghost" size="icon-sm" aria-label="切换主题">
								{theme === "dark" ? <Moon /> : theme === "light" ? <Sun /> : <Monitor />}
							</Button>
						</DropdownMenuTrigger>
						<DropdownMenuContent align="end">
							<DropdownMenuLabel>外观</DropdownMenuLabel>
							<DropdownMenuItem onClick={() => setThemePreference("dark")}>
								<Moon /> 暗色
							</DropdownMenuItem>
							<DropdownMenuItem onClick={() => setThemePreference("light")}>
								<Sun /> 亮色
							</DropdownMenuItem>
							<DropdownMenuItem onClick={() => setThemePreference("system")}>
								<Monitor /> 跟随系统
							</DropdownMenuItem>
						</DropdownMenuContent>
					</DropdownMenu>

					<DropdownMenu>
						<DropdownMenuTrigger asChild>
							<Button variant="ghost" size="sm" className="gap-2 px-1.5">
								{scope === "user" ? (
									<UserAvatar name={identity} avatar={me?.avatar ?? null} size="sm" />
								) : (
									<span className="flex size-6 items-center justify-center rounded-md bg-muted text-[10px] font-semibold">
										ADM
									</span>
								)}
								<span className="hidden max-w-32 truncate text-sm sm:inline">{identity}</span>
								<ChevronsUpDown className="size-3.5 text-muted-foreground" />
							</Button>
						</DropdownMenuTrigger>
						<DropdownMenuContent align="end" className="w-52">
							<DropdownMenuLabel className="truncate">
								{scope === "admin"
									? adminToken
										? "管理员"
										: "未登录"
									: (me?.username ?? "未登录")}
							</DropdownMenuLabel>
							<DropdownMenuSeparator />
							{scope === "user" ? (
								<DropdownMenuItem onClick={() => void navigate("/profile")}>
									<UserCircle /> 我的资料
								</DropdownMenuItem>
							) : (
								<DropdownMenuItem onClick={() => void navigate("/")}>
									<UserCircle /> 前往选手端
								</DropdownMenuItem>
							)}
							<DropdownMenuSeparator />
							<DropdownMenuItem variant="destructive" onClick={logout}>
								<LogOut /> 退出登录
							</DropdownMenuItem>
						</DropdownMenuContent>
					</DropdownMenu>
				</header>

				<main className="min-w-0 flex-1 pb-16 md:pb-0">
					<Outlet />
				</main>
			</SidebarInset>

			{/* 移动端底部导航 */}
			<nav className="fixed inset-x-0 bottom-0 z-30 flex border-t bg-background/95 backdrop-blur md:hidden">
				{mobileNav.map((item) => {
					const Icon = item.icon;
					const active = isActivePath(location.pathname, item);
					return (
						<NavLink
							key={item.to}
							to={item.to}
							end={item.end}
							className={cn(
								"flex flex-1 flex-col items-center gap-0.5 py-2 text-[11px]",
								active ? "text-foreground" : "text-muted-foreground",
							)}
						>
							<Icon className="size-4" />
							{item.label}
						</NavLink>
					);
				})}
			</nav>
		</SidebarProvider>
	);
}

/** 选手端外壳。 */
export function PlayerShell() {
	return (
		<AppShell
			brand="FloatCTF"
			brandSubtitle="Field · 选手工作区"
			navGroups={playerNavGroups}
			mobileNav={playerMobileNav}
			scope="user"
		/>
	);
}

/** 管理端外壳。 */
export function AdminShell() {
	return (
		<AppShell
			brand="FloatCTF"
			brandSubtitle="Control Room · 运维控制台"
			navGroups={adminNavGroups}
			mobileNav={adminMobileNav}
			scope="admin"
		/>
	);
}
