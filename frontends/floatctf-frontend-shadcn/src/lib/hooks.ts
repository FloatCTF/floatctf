import { useCallback, useEffect, useRef, useState } from "react";

/** 输入框防抖（搜索）。 */
export function useDebouncedValue<T>(value: T, delay = 300): T {
	const [debounced, setDebounced] = useState(value);
	useEffect(() => {
		const timer = setTimeout(() => setDebounced(value), delay);
		return () => clearTimeout(timer);
	}, [value, delay]);
	return debounced;
}

export function useDebouncedCallback<A extends unknown[]>(
	callback: (...args: A) => void,
	delay = 300,
): (...args: A) => void {
	const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
	const latest = useRef(callback);
	latest.current = callback;
	useEffect(
		() => () => {
			if (timer.current) clearTimeout(timer.current);
		},
		[],
	);
	return useCallback(
		(...args: A) => {
			if (timer.current) clearTimeout(timer.current);
			timer.current = setTimeout(() => latest.current(...args), delay);
		},
		[delay],
	);
}

/** 每 `intervalMs` 滴答一次的「现在」，用于倒计时 / 相对时间。 */
export function useNow(intervalMs = 30_000): Date {
	const [now, setNow] = useState(() => new Date());
	useEffect(() => {
		const timer = setInterval(() => setNow(new Date()), intervalMs);
		return () => clearInterval(timer);
	}, [intervalMs]);
	return now;
}

export function useMediaQuery(query: string): boolean {
	const [matches, setMatches] = useState(() => {
		if (typeof window === "undefined" || !window.matchMedia) return false;
		return window.matchMedia(query).matches;
	});
	useEffect(() => {
		if (typeof window === "undefined" || !window.matchMedia) return;
		const media = window.matchMedia(query);
		const onChange = () => setMatches(media.matches);
		onChange();
		media.addEventListener("change", onChange);
		return () => media.removeEventListener("change", onChange);
	}, [query]);
	return matches;
}

/** 复制到剪贴板（带 1.5s 的「已复制」反馈状态）。 */
export function useCopyToClipboard(): {
	copied: boolean;
	copy: (text: string) => Promise<boolean>;
} {
	const [copied, setCopied] = useState(false);
	const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
	useEffect(
		() => () => {
			if (timer.current) clearTimeout(timer.current);
		},
		[],
	);
	const copy = useCallback(async (text: string) => {
		try {
			if (navigator.clipboard?.writeText) {
				await navigator.clipboard.writeText(text);
			} else {
				const area = document.createElement("textarea");
				area.value = text;
				area.style.position = "fixed";
				area.style.opacity = "0";
				document.body.appendChild(area);
				area.select();
				document.execCommand("copy");
				area.remove();
			}
			setCopied(true);
			if (timer.current) clearTimeout(timer.current);
			timer.current = setTimeout(() => setCopied(false), 1500);
			return true;
		} catch {
			setCopied(false);
			return false;
		}
	}, []);
	return { copied, copy };
}

/** 受控开关（对话框 / 抽屉）。 */
export function useDisclosure(initial = false): {
	open: boolean;
	setOpen: (value: boolean) => void;
	onOpen: () => void;
	onClose: () => void;
	toggle: () => void;
} {
	const [open, setOpen] = useState(initial);
	return {
		open,
		setOpen,
		onOpen: useCallback(() => setOpen(true), []),
		onClose: useCallback(() => setOpen(false), []),
		toggle: useCallback(() => setOpen((value) => !value), []),
	};
}

/** 页面标题（`document.title`）——不侵入路由层。 */
export function useDocumentTitle(title: string): void {
	useEffect(() => {
		const previous = document.title;
		document.title = title;
		return () => {
			document.title = previous;
		};
	}, [title]);
}
