import { useRouter, useRouterState } from "@tanstack/react-router";
import type { NavigateOptions } from "@tanstack/react-router";
import {
	createContext,
	useCallback,
	useContext,
	useEffect,
	useRef,
	useState,
} from "react";
import type { CoordinatorNavigateOptions } from "./navigation-types";
import type { NavigationPhase } from "./navigation-types";

// ── 乐观进度条步进 ──────────────────────────────────────────────
// 预加载期间从约 8% 缓增到 91%，完成时跳到 100%。
const TRICKLE_STEPS = [8, 20, 35, 50, 65, 75, 82, 88, 91];
const TRICKLE_INTERVAL_MS = 80;

// 由 router pending 驱动（前进/后退）的缓增步进
const PENDING_TRICKLE_STEPS = [30, 50, 65, 78, 85, 90];
const PENDING_TRICKLE_INTERVAL_MS = 120;

// 显示 100% 后多久开始淡出
const COMPLETE_ANIMATION_MS = 150;

// ── 上下文值 ──────────────────────────────────────────────────────────────

export interface NavigationContextValue {
	phase: NavigationPhase;
	progress: number;
	visible: boolean;
	navigateWithTransition: (opts: CoordinatorNavigateOptions) => void;
}

export const NavigationContext = createContext<NavigationContextValue | null>(
	null,
);

// ── 模块级事务计数器（同步，React 外） ──────────────
// 用于在不重渲染的情况下检测被覆盖的事务。
let nextTransactionId = 0;

// ── Provider ──────────────────────────────────────────────────────────────────

export function NavigationProvider({
	children,
}: {
	children: React.ReactNode;
}) {
	const router = useRouter();
	const routerStatus = useRouterState({ select: (s) => s.status });

	// 用于渲染的 React state
	const [phase, setPhase] = useState<NavigationPhase>("idle");
	const [progress, setProgress] = useState(0);
	const [visible, setVisible] = useState(false);

	// ref——可变，不触发重渲染
	const activeTransactionRef = useRef(0);
	const trickleIvRef = useRef<ReturnType<typeof setInterval> | null>(null);
	const hideTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
	const routerDrivenIvRef = useRef<ReturnType<typeof setInterval> | null>(null);
	const routerDrivenActiveRef = useRef(false);

	// ── 辅助函数 ────────────────────────────────────────────────────────────────

	const cleanupTimers = useCallback(() => {
		if (trickleIvRef.current) {
			clearInterval(trickleIvRef.current);
			trickleIvRef.current = null;
		}
		if (hideTimeoutRef.current) {
			clearTimeout(hideTimeoutRef.current);
			hideTimeoutRef.current = null;
		}
		if (routerDrivenIvRef.current) {
			clearInterval(routerDrivenIvRef.current);
			routerDrivenIvRef.current = null;
		}
	}, []);

	const resetState = useCallback(() => {
		cleanupTimers();
		setPhase("idle");
		setProgress(0);
		setVisible(false);
		routerDrivenActiveRef.current = false;
	}, [cleanupTimers]);

	const startTrickle = useCallback(() => {
		cleanupTimers();
		let stepIndex = 0;
		setProgress(TRICKLE_STEPS[0]);
		trickleIvRef.current = setInterval(() => {
			stepIndex++;
			if (stepIndex >= TRICKLE_STEPS.length) {
				clearInterval(trickleIvRef.current!);
				trickleIvRef.current = null;
				return;
			}
			setProgress(TRICKLE_STEPS[stepIndex]);
		}, TRICKLE_INTERVAL_MS);
	}, [cleanupTimers]);

	/**
	 * 跳到 100%，动画结束后再重置。
	 * 供 router-pending 驱动的进度条使用（前进/后退）。
	 */
	const completeProgress = useCallback(() => {
		if (trickleIvRef.current) {
			clearInterval(trickleIvRef.current);
			trickleIvRef.current = null;
		}
		if (routerDrivenIvRef.current) {
			clearInterval(routerDrivenIvRef.current);
			routerDrivenIvRef.current = null;
		}
		setProgress(100);
		const txAtSchedule = activeTransactionRef.current;
		hideTimeoutRef.current = setTimeout(() => {
			hideTimeoutRef.current = null;
			// 若动画期间已有协调器事务启动则退出——
			// 此时重置会冲掉新事务的进度。
			if (activeTransactionRef.current !== txAtSchedule) return;
			resetState();
		}, COMPLETE_ANIMATION_MS + 100);
	}, [resetState]);

	// ── router-pending 驱动进度（前进/后退 / 非协调导航） ─────
	// 仅在没有活跃协调器事务时启用。
	useEffect(() => {
		if (phase !== "idle") {
			// 协调器活跃——清理 router 驱动的 interval
			if (routerDrivenIvRef.current) {
				clearInterval(routerDrivenIvRef.current);
				routerDrivenIvRef.current = null;
			}
			routerDrivenActiveRef.current = false;
			return;
		}

		if (routerStatus === "pending") {
			// 启动 router 驱动的进度条
			routerDrivenActiveRef.current = true;
			setVisible(true);
			setProgress(PENDING_TRICKLE_STEPS[0]);
			let step = 0;
			routerDrivenIvRef.current = setInterval(() => {
				step++;
				if (step >= PENDING_TRICKLE_STEPS.length) {
					clearInterval(routerDrivenIvRef.current!);
					routerDrivenIvRef.current = null;
					return;
				}
				setProgress(PENDING_TRICKLE_STEPS[step]);
			}, PENDING_TRICKLE_INTERVAL_MS);
		} else if (routerStatus === "idle" && routerDrivenActiveRef.current) {
			// router pending 完成——收尾进度条
			routerDrivenActiveRef.current = false;
			completeProgress();
		}

		return () => {
			if (routerDrivenIvRef.current) {
				clearInterval(routerDrivenIvRef.current);
				routerDrivenIvRef.current = null;
			}
		};
	}, [routerStatus, phase, completeProgress]);

	// ── 卸载清理 ─────────────────────────────────────────────────────
	useEffect(() => {
		return () => {
			cleanupTimers();
		};
	}, [cleanupTimers]);

	// ── 核心：带过渡的导航（提交前预加载） ─────────────────
	const navigateWithTransition = useCallback(
		(opts: CoordinatorNavigateOptions) => {
			const transactionId = ++nextTransactionId;
			activeTransactionRef.current = transactionId;
			const startHref = router.state.location.href;

			// 转为 router 类型化的 NavigateOptions
			const routerOpts = opts as unknown as NavigateOptions;

			// 进入预加载阶段
			setPhase("preloading");
			setVisible(true);
			startTrickle();

			router.preloadRoute(routerOpts).then(
				() => {
					// ── 预加载成功 ──
					if (transactionId !== activeTransactionRef.current) return;

					// location 可能已变（鉴权拦截器重定向）
					if (router.state.location.href !== startHref) {
						resetState();
						return;
					}

					// 动画到 100%，再提交导航
					setPhase("committing");
					setProgress(100);

					setTimeout(() => {
						if (transactionId !== activeTransactionRef.current) return;

						// 提交前再检查 location（用户可能按了后退）
						if (router.state.location.href !== startHref) {
							resetState();
							return;
						}

						router.navigate(routerOpts).then(
							() => {
								if (transactionId === activeTransactionRef.current) {
									resetState();
								}
							},
							() => {
								if (transactionId === activeTransactionRef.current) {
									resetState();
								}
							},
						);
					}, COMPLETE_ANIMATION_MS);
				},
				() => {
					// ── 预加载失败 ──
					if (transactionId !== activeTransactionRef.current) return;

					// location 已变（鉴权重定向）——静默中止
					if (router.state.location.href !== startHref) {
						resetState();
						return;
					}

					// 回退：交给 router 常规错误/加载系统
					resetState();
					router.navigate(routerOpts).catch(() => {});
				},
			);
		},
		[router, startTrickle, resetState],
	);

	return (
		<NavigationContext.Provider
			value={{ phase, progress, visible, navigateWithTransition }}
		>
			{children}
		</NavigationContext.Provider>
	);
}

// ── Hook ──────────────────────────────────────────────────────────────────────

export function useNavigation(): NavigationContextValue {
	const ctx = useContext(NavigationContext);
	if (!ctx) {
		throw new Error("useNavigation must be used within a NavigationProvider");
	}
	return ctx;
}
