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

// ── Optimistic progress bar steps ──────────────────────────────────────────────
// Trickle from ~8% → 91% during preload, then jump to 100% on complete.
const TRICKLE_STEPS = [8, 20, 35, 50, 65, 75, 82, 88, 91];
const TRICKLE_INTERVAL_MS = 80;

// Router-pending-driven (back/forward) trickle steps
const PENDING_TRICKLE_STEPS = [30, 50, 65, 78, 85, 90];
const PENDING_TRICKLE_INTERVAL_MS = 120;

// How long to show 100% before starting fade-out
const COMPLETE_ANIMATION_MS = 150;

// ── Context value ──────────────────────────────────────────────────────────────

export interface NavigationContextValue {
	phase: NavigationPhase;
	progress: number;
	visible: boolean;
	navigateWithTransition: (opts: CoordinatorNavigateOptions) => void;
}

export const NavigationContext = createContext<NavigationContextValue | null>(
	null,
);

// ── Module-level transaction counter (synchronous, outside React) ──────────────
// Used to detect superseded transactions without re-rendering.
let nextTransactionId = 0;

// ── Provider ──────────────────────────────────────────────────────────────────

export function NavigationProvider({
	children,
}: {
	children: React.ReactNode;
}) {
	const router = useRouter();
	const routerStatus = useRouterState({ select: (s) => s.status });

	// React state for rendering
	const [phase, setPhase] = useState<NavigationPhase>("idle");
	const [progress, setProgress] = useState(0);
	const [visible, setVisible] = useState(false);

	// Refs — mutable, not triggering re-renders
	const activeTransactionRef = useRef(0);
	const trickleIvRef = useRef<ReturnType<typeof setInterval> | null>(null);
	const hideTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
	const routerDrivenIvRef = useRef<ReturnType<typeof setInterval> | null>(null);
	const routerDrivenActiveRef = useRef(false);

	// ── Helpers ────────────────────────────────────────────────────────────────

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
	 * Jump to 100%, then reset after the animation completes.
	 * Used by router-pending-driven progress (back/forward).
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
			// Bail if a coordinator transaction started while we were animating —
			// resetting now would clobber the new transaction's progress.
			if (activeTransactionRef.current !== txAtSchedule) return;
			resetState();
		}, COMPLETE_ANIMATION_MS + 100);
	}, [resetState]);

	// ── Router-pending-driven progress (back/forward / non-coordinated nav) ─────
	// Only engages when there is no active coordinator transaction.
	useEffect(() => {
		if (phase !== "idle") {
			// Coordinator active — clean up any router-driven interval
			if (routerDrivenIvRef.current) {
				clearInterval(routerDrivenIvRef.current);
				routerDrivenIvRef.current = null;
			}
			routerDrivenActiveRef.current = false;
			return;
		}

		if (routerStatus === "pending") {
			// Start router-driven progress bar
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
			// Router pending completed — finish the bar
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

	// ── Cleanup on unmount ─────────────────────────────────────────────────────
	useEffect(() => {
		return () => {
			cleanupTimers();
		};
	}, [cleanupTimers]);

	// ── Core: navigate with transition (preload before commit) ─────────────────
	const navigateWithTransition = useCallback(
		(opts: CoordinatorNavigateOptions) => {
			const transactionId = ++nextTransactionId;
			activeTransactionRef.current = transactionId;
			const startHref = router.state.location.href;

			// Cast to the router's typed NavigateOptions
			const routerOpts = opts as unknown as NavigateOptions;

			// Enter preloading phase
			setPhase("preloading");
			setVisible(true);
			startTrickle();

			router.preloadRoute(routerOpts).then(
				() => {
					// ── Preload succeeded ──
					if (transactionId !== activeTransactionRef.current) return;

					// Location may have changed (auth redirect via interceptor)
					if (router.state.location.href !== startHref) {
						resetState();
						return;
					}

					// Animate to 100%, then commit
					setPhase("committing");
					setProgress(100);

					setTimeout(() => {
						if (transactionId !== activeTransactionRef.current) return;

						// Re-check location before commit (user may have pressed Back)
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
					// ── Preload failed ──
					if (transactionId !== activeTransactionRef.current) return;

					// Location changed (auth redirect) — abort silently
					if (router.state.location.href !== startHref) {
						resetState();
						return;
					}

					// Fallback: let the normal router error/loading system handle it
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
