/**
 * 主题偏好 —— 归本前端所有（平台不关心）。
 * 暗色是默认（控制台 / 比赛场景），浅色与跟随系统都支持；写入 localStorage。
 */

import { useSyncExternalStore } from "react";

export type ThemePreference = "dark" | "light" | "system";

const STORAGE_KEY = "shadcn.theme";
const MEDIA = "(prefers-color-scheme: dark)";

function readStored(): ThemePreference {
	try {
		const value = window.localStorage.getItem(STORAGE_KEY);
		if (value === "dark" || value === "light" || value === "system") return value;
	} catch {
		/* 隐私模式：退化为默认值 */
	}
	return "dark";
}

let preference: ThemePreference = typeof window === "undefined" ? "dark" : readStored();
const listeners = new Set<() => void>();

function prefersDark(): boolean {
	if (typeof window === "undefined" || !window.matchMedia) return true;
	return window.matchMedia(MEDIA).matches;
}

function resolved(pref: ThemePreference): "dark" | "light" {
	if (pref === "system") return prefersDark() ? "dark" : "light";
	return pref;
}

/** 把主题写到 <html class="dark">，与 globals.css 的变量切换保持一致。 */
function apply(): void {
	if (typeof document === "undefined") return;
	const theme = resolved(preference);
	const root = document.documentElement;
	root.classList.toggle("dark", theme === "dark");
	root.style.colorScheme = theme;
}

function emit(): void {
	for (const listener of listeners) listener();
}

export function getThemePreference(): ThemePreference {
	return preference;
}

export function getResolvedTheme(): "dark" | "light" {
	return resolved(preference);
}

export function setThemePreference(next: ThemePreference): void {
	preference = next;
	try {
		window.localStorage.setItem(STORAGE_KEY, next);
	} catch {
		/* 忽略存储失败 */
	}
	apply();
	emit();
}

/** 在 mount 时调用一次：应用已存偏好并跟随系统变化。 */
export function initTheme(): () => void {
	apply();
	if (typeof window === "undefined" || !window.matchMedia) return () => {};
	const media = window.matchMedia(MEDIA);
	const onChange = () => {
		if (preference === "system") {
			apply();
			emit();
		}
	};
	media.addEventListener("change", onChange);
	return () => media.removeEventListener("change", onChange);
}

export function useTheme(): ThemePreference {
	return useSyncExternalStore(
		(listener) => {
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
		() => preference,
		() => "dark" as ThemePreference,
	);
}

export function useResolvedTheme(): "dark" | "light" {
	return useSyncExternalStore(
		(listener) => {
			listeners.add(listener);
			return () => listeners.delete(listener);
		},
		() => resolved(preference),
		() => "dark" as const,
	);
}
