/**
 * 命令面板 —— 键盘优先入口（⌘K / Ctrl-K）。
 *
 * 命令来源：
 * 1. 导航命令（由 `nav.ts` 的信息架构自动派生）；
 * 2. 页面通过 `useCommands([...])` 注册的**上下文命令**（提交 flag / 启动实例 / 新建赛事…）。
 *    卸载时自动撤销，不会在别的页面里残留。
 */

import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { useNavigate } from "react-router";
import { create } from "zustand";

import {
	CommandDialog,
	CommandEmpty,
	CommandGroup,
	CommandInput,
	CommandItem,
	CommandList,
	CommandSeparator,
	CommandShortcut,
} from "~/components/ui/command";
import { useIsAdminAuthenticated } from "~/auth/store";
import { adminNav, flatNav, playerNav } from "./nav";

export interface AppCommand {
	id: string;
	label: string;
	group?: string;
	keywords?: string;
	icon?: ReactNode;
	shortcut?: string;
	run: () => void | Promise<void>;
}

interface CommandStore {
	contextual: AppCommand[];
	register: (commands: AppCommand[]) => void;
	unregister: (ids: string[]) => void;
}

const useCommandStore = create<CommandStore>((set) => ({
	contextual: [],
	register: (commands) =>
		set((state) => ({
			contextual: [...state.contextual.filter((item) => !commands.some((c) => c.id === item.id)), ...commands],
		})),
	unregister: (ids) =>
		set((state) => ({ contextual: state.contextual.filter((item) => !ids.includes(item.id)) })),
}));

/** 页面注册上下文命令；`deps` 变化会重新注册。 */
export function useCommands(commands: AppCommand[], deps: unknown[] = []): void {
	const register = useCommandStore((state) => state.register);
	const unregister = useCommandStore((state) => state.unregister);
	const ids = useMemo(() => commands.map((command) => command.id), [commands]);
	useEffect(() => {
		register(commands);
		return () => unregister(ids);
		// biome-ignore lint/correctness/useExhaustiveDependencies: 由调用方通过 deps 控制
	}, [register, unregister, ids.join("|"), ...deps]);
}

interface PaletteState {
	open: boolean;
	setOpen: (open: boolean) => void;
	toggle: () => void;
}

const usePaletteStore = create<PaletteState>((set) => ({
	open: false,
	setOpen: (open) => set({ open }),
	toggle: () => set((state) => ({ open: !state.open })),
}));

export function useCommandPalette(): PaletteState {
	return usePaletteStore();
}

/** 全局快捷键：⌘K / Ctrl-K 打开命令面板。 */
export function CommandPalette(): ReactNode {
	const { open, setOpen, toggle } = usePaletteStore();
	const contextual = useCommandStore((state) => state.contextual);
	const navigate = useNavigate();
	const isAdmin = useIsAdminAuthenticated();

	useEffect(() => {
		const onKeyDown = (event: KeyboardEvent) => {
			if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
				event.preventDefault();
				toggle();
			}
		};
		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, [toggle]);

	const go = useCallback(
		(to: string) => {
			setOpen(false);
			void navigate(to);
		},
		[navigate, setOpen],
	);

	const navigationGroups = useMemo(() => {
		const sortNav = (items: ReturnType<typeof flatNav>) =>
			[...items].sort((a, b) => a.label.localeCompare(b.label, "zh-CN"));
		if (isAdmin) {
			return [
				{ label: "管理端导航", items: sortNav(flatNav(adminNav)) },
				{ label: "选手端导航", items: sortNav(flatNav(playerNav)) },
			];
		}
		return [{ label: "导航", items: sortNav(flatNav(playerNav)) }];
	}, [isAdmin]);

	const [shortcutHint, setShortcutHint] = useState("⌘K");
	useEffect(() => {
		if (typeof navigator !== "undefined" && !/Mac|iPhone|iPad/.test(navigator.platform)) {
			setShortcutHint("Ctrl K");
		}
	}, []);

	return (
		<CommandDialog open={open} onOpenChange={setOpen} title="命令面板" description="搜索页面与操作">
			<CommandInput placeholder="搜索页面、赛事、题目或操作…" />
			<CommandList>
				<CommandEmpty>没有匹配结果。</CommandEmpty>

				{contextual.length > 0 ? (
					<>
						<CommandGroup heading="当前页面操作">
							{contextual.map((command) => (
								<CommandItem
									key={command.id}
									value={`${command.label} ${command.keywords ?? ""}`}
									onSelect={() => {
										setOpen(false);
										void command.run();
									}}
								>
									{command.icon}
									<span>{command.label}</span>
									{command.shortcut ? <CommandShortcut>{command.shortcut}</CommandShortcut> : null}
								</CommandItem>
							))}
						</CommandGroup>
						<CommandSeparator />
					</>
				) : null}

				{navigationGroups.map((group) => (
					<CommandGroup key={group.label} heading={group.label}>
						{group.items.map((item) => {
							const Icon = item.icon;
							return (
								<CommandItem
									key={item.to}
									value={`${item.label} ${item.to} ${item.description ?? ""}`}
									onSelect={() => go(item.to)}
								>
									<Icon className="size-4" />
									<span>{item.label}</span>
									<CommandShortcut>{item.to}</CommandShortcut>
								</CommandItem>
							);
						})}
					</CommandGroup>
				))}

				<CommandSeparator />
				<CommandGroup heading="提示">
					<CommandItem value="快捷键 keyboard" onSelect={() => setOpen(false)}>
						<span>打开命令面板</span>
						<CommandShortcut>{shortcutHint}</CommandShortcut>
					</CommandItem>
				</CommandGroup>
			</CommandList>
		</CommandDialog>
	);
}

/** 顶栏里的「搜索 / 命令」触发器。 */
export function CommandTrigger({ className }: { className?: string }) {
	const { setOpen } = usePaletteStore();
	return (
		<button
			type="button"
			onClick={() => setOpen(true)}
			className={className}
			aria-label="打开命令面板"
		>
			<span className="inline-flex items-center gap-2">
				<SearchIcon />
				<span className="hidden sm:inline">搜索 / 命令</span>
			</span>
			<kbd className="hidden items-center gap-0.5 rounded border bg-muted px-1.5 font-mono text-[10px] text-muted-foreground sm:inline-flex">
				⌘K
			</kbd>
		</button>
	);
}

function SearchIcon() {
	return (
		<svg
			xmlns="http://www.w3.org/2000/svg"
			viewBox="0 0 24 24"
			fill="none"
			stroke="currentColor"
			strokeWidth="2"
			strokeLinecap="round"
			strokeLinejoin="round"
			className="size-4"
			aria-hidden="true"
		>
			<circle cx="11" cy="11" r="8" />
			<path d="m21 21-4.3-4.3" />
		</svg>
	);
}
