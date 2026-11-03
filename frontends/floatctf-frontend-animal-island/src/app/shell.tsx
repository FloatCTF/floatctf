/**
 * 应用外壳：顶栏 + 侧栏 + 内容区。
 *
 * 交互模型（本前端自主设计，不复刻 Default）：
 * - 顶栏左侧是**工作区切换**（岛屿 / 控制台），不是两棵互不相干的导航树；
 * - 顶栏右侧是当前账号与登出；
 * - `⌘/Ctrl+K` 打开快速跳转（Drawer + Input 过滤）—— 刻意不引入额外依赖，
 *   一个 Drawer 加一个受控 Input 就够；
 * - 侧栏只在**有权限**时显示控制台入口；权限来自 `/me`，仅作显隐。
 */
import { useEffect, useMemo, useState } from "react";
import { NavLink, Outlet, useLocation, useNavigate } from "react-router";
import {
	Avatar,
	Button,
	Card,
	Divider,
	Drawer,
	Input,
	Tag,
	Tooltip,
} from "animal-island-ui";
import {
	CompassIcon,
	FlagIcon,
	ArrowRightIcon,
	SearchIcon,
} from "naive-icons";

import { useRuntime } from "~/app/runtime-context";
import { useSession, hasPlatformAccess } from "~/auth/store";
import { ISLAND_NAV, flatNav, visibleConsoleNav } from "~/app/nav";

function useWorkspace(): "island" | "console" {
	const { pathname } = useLocation();
	return pathname.startsWith("/console") ? "console" : "island";
}

function TopBar({ onOpenJump }: { onOpenJump: () => void }) {
	const runtime = useRuntime();
	const navigate = useNavigate();
	const workspace = useWorkspace();
	const me = useSession((s) => s.me);
	const clear = useSession((s) => s.clear);

	const canConsole = hasPlatformAccess(me);

	return (
		<header className="ai-topbar">
			<div className="ai-topbar-brand">
				<span className="ai-topbar-brand-dot" aria-hidden="true" />
				<span>FloatCTF</span>
				<Tag size="small" variant="soft" color="app-teal">
					动物岛
				</Tag>
			</div>

			<div className="ai-row" style={{ gap: 4 }}>
				<Button
					size="small"
					type={workspace === "island" ? "primary" : "text"}
					icon={<FlagIcon size={16} />}
					onClick={() => void navigate("/island")}
				>
					岛屿
				</Button>
				{/* 控制台入口只在确实有平台能力时出现：这不是授权判定，
				    只是不给普通选手展示一个点了就 403 的入口。 */}
				{canConsole ? (
					<Button
						size="small"
						type={workspace === "console" ? "primary" : "text"}
						icon={<CompassIcon size={16} />}
						onClick={() => void navigate("/console")}
					>
						控制台
					</Button>
				) : null}
			</div>

			<div className="ai-topbar-spacer" />

			<Tooltip title="快速跳转（⌘/Ctrl+K）">
				<Button size="small" type="text" icon={<SearchIcon size={16} />} onClick={onOpenJump}>
					跳转
				</Button>
			</Tooltip>

			<div className="ai-row" style={{ gap: 8 }}>
				<Avatar size="small" src={me?.avatar ?? undefined}>
					{me?.nickname?.[0] ?? me?.username?.[0] ?? "?"}
				</Avatar>
				<span className="ai-strong" style={{ maxWidth: 140, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
					{me?.nickname ?? me?.username ?? "未登录"}
				</span>
				<Tooltip title={`后端契约 v${runtime.frontendVersion}`}>
					<span className="ai-muted" style={{ fontSize: 12 }}>
						v{runtime.frontendVersion}
					</span>
				</Tooltip>
				<Button
					size="small"
					type="text"
					danger
					icon={<ArrowRightIcon size={16} />}
					onClick={() => {
						clear();
						void navigate("/login", { replace: true });
					}}
				>
					登出
				</Button>
			</div>
		</header>
	);
}

function SideBar() {
	const me = useSession((s) => s.me);
	const workspace = useWorkspace();
	const permissionSet = useMemo(
		() => new Set(me?.permissions ?? []),
		[me?.permissions],
	);

	// 岛屿导航对所有登录用户可见；控制台按权限裁剪。
	const groups = workspace === "console" ? visibleConsoleNav(permissionSet) : ISLAND_NAV;

	return (
		<nav className="ai-sidebar" aria-label={workspace === "console" ? "控制台导航" : "岛屿导航"}>
			{groups.map((group) => (
				<div key={group.label} className="ai-col" style={{ gap: 2 }}>
					<div className="ai-nav-group-label">{group.label}</div>
					{group.items.map((item) => {
						const Icon = item.icon;
						return (
							<NavLink
								key={item.to}
								to={item.to}
								end={"end" in item ? Boolean(item.end) : false}
								className={({ isActive }) =>
									isActive ? "ai-nav-item ai-nav-item-active" : "ai-nav-item"
								}
							>
								<Icon size={18} />
								<span className="ai-nav-item-label">{item.label}</span>
							</NavLink>
						);
					})}
				</div>
			))}
			{groups.length === 0 ? (
				<div className="ai-muted" style={{ padding: 12 }}>
					这个工作区没有你可用的入口。
				</div>
			) : null}
		</nav>
	);
}

/** ⌘/Ctrl+K 快速跳转。 */
function QuickJump({ open, onClose }: { open: boolean; onClose: () => void }) {
	const me = useSession((s) => s.me);
	const navigate = useNavigate();
	const [query, setQuery] = useState("");

	const permissionSet = useMemo(() => new Set(me?.permissions ?? []), [me?.permissions]);
	const all = useMemo(() => flatNav(permissionSet), [permissionSet]);
	const results = useMemo(() => {
		const q = query.trim().toLowerCase();
		if (!q) return all.slice(0, 12);
		return all
			.filter((i) => `${i.label}${i.to}${i.group}`.toLowerCase().includes(q))
			.slice(0, 20);
	}, [all, query]);

	useEffect(() => {
		if (!open) setQuery("");
	}, [open]);

	return (
		<Drawer open={open} onClose={onClose} title="快速跳转" placement="right" width={420}>
			<div className="ai-col">
				<Input
					allowClear
					autoFocus
					placeholder="搜索页面…"
					prefix={<SearchIcon size={16} />}
					value={query}
					onChange={(e) => setQuery(e.target.value)}
				/>
				{results.length === 0 ? (
					<div className="ai-muted" style={{ padding: 12 }}>
						没有匹配的页面。
					</div>
				) : (
					results.map((item) => {
						const Icon = item.icon;
						return (
							<Button
								key={item.to}
								type="text"
								block
								icon={<Icon size={18} />}
								onClick={() => {
									void navigate(item.to);
									onClose();
								}}
							>
								<span className="ai-row-between" style={{ width: "100%" }}>
									<span>{item.label}</span>
									<span className="ai-muted" style={{ fontSize: 12 }}>
										{item.group}
									</span>
								</span>
							</Button>
						);
					})
				)}
			</div>
		</Drawer>
	);
}

export function Shell() {
	const [jumpOpen, setJumpOpen] = useState(false);

	useEffect(() => {
		const onKey = (e: KeyboardEvent) => {
			if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
				e.preventDefault();
				setJumpOpen((v) => !v);
			}
		};
		window.addEventListener("keydown", onKey);
		return () => window.removeEventListener("keydown", onKey);
	}, []);

	return (
		<div className="ai-app">
			<TopBar onOpenJump={() => setJumpOpen(true)} />
			<div className="ai-body">
				<SideBar />
				<main className="ai-main ai-scroll">
					<div className="ai-main-inner">
						<Outlet />
					</div>
				</main>
			</div>
			<QuickJump open={jumpOpen} onClose={() => setJumpOpen(false)} />
		</div>
	);
}

/** 页面级容器：统一标题区，避免每页自己拼标题样式。 */
export function PageHeader({
	title,
	description,
	extra,
}: {
	title: string;
	description?: string;
	extra?: React.ReactNode;
}) {
	return (
		<Card>
			<div className="ai-row-between">
				<div className="ai-col" style={{ gap: 2 }}>
					<h2 style={{ margin: 0, fontSize: 20, fontWeight: 800 }}>{title}</h2>
					{description ? <span className="ai-muted">{description}</span> : null}
				</div>
				{extra ? <div className="ai-row">{extra}</div> : null}
			</div>
			<Divider />
		</Card>
	);
}
