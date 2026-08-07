import { useEffect, useState } from "react";
import { useNavigation } from "./NavigationContext";

/**
 * 2px progress bar at the top of the viewport.
 * Positioned above content, below tooltips (z-index: 1000).
 * Uses CSS transform: scaleX() for smooth width animation.
 * Respects prefers-reduced-motion.
 */
export function NavigationProgress() {
	const { visible, progress } = useNavigation();
	const prefersReduced = usePrefersReducedMotion();

	return (
		<div
			aria-hidden={!visible}
			className="floatctf-nav-progress"
			style={{
				transform: `scaleX(${progress / 100})`,
				opacity: visible ? 1 : 0,
				transition: prefersReduced
					? "none"
					: "transform 0.15s ease, opacity 0.2s ease",
			}}
		/>
	);
}

function usePrefersReducedMotion(): boolean {
	const [prefersReduced, setPrefersReduced] = useState(
		() => window.matchMedia("(prefers-reduced-motion: reduce)").matches,
	);

	useEffect(() => {
		const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
		const handler = (e: MediaQueryListEvent) => setPrefersReduced(e.matches);
		mq.addEventListener("change", handler);
		return () => mq.removeEventListener("change", handler);
	}, []);

	return prefersReduced;
}
