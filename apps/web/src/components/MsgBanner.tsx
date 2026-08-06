import { Banner } from "@primer/react/experimental";
import { useReactive } from "ahooks";
import type { AxiosError } from "axios";
import { useCallback, useEffect, useRef } from "react";

export type BannerVariant =
	| "critical"
	| "info"
	| "success"
	| "upsell"
	| "warning";

export interface UseMsgBannerOptions {
	isShown?: boolean; // 是否默认显示
	description?: string; // 默认描述
	variant?: BannerVariant; // 默认类型
	duration?: number; // 自动隐藏时间（ms）
}

export const useMsgBanner = (options: UseMsgBannerOptions = {}) => {
	// 用传入的初始值覆盖默认值
	const mutationBanner = useReactive({
		isShown: options.isShown ?? false,
		description: options.description ?? "Something here",
		variant: options.variant ?? ("info" as BannerVariant),
	});
	const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
	const duration = options.duration ?? 3000; // 默认 5 秒自动隐藏
	const clearTimer = useCallback(() => {
		if (timerRef.current) {
			clearTimeout(timerRef.current);
			timerRef.current = null;
		}
	}, []);

	const showBanner = (variant: BannerVariant, description: string) => {
		mutationBanner.isShown = true;
		mutationBanner.variant = variant;
		mutationBanner.description = description;

		clearTimer();
		timerRef.current = setTimeout(() => {
			mutationBanner.isShown = false;
			timerRef.current = null;
		}, duration);
	};

	const showErrorBanner = (error: unknown) => {
		const msg =
			(error as AxiosError<{ message: string }>)?.response?.data?.message ||
			(error as Error).message ||
			"Unknown error";
		showBanner("critical", msg);
	};

	const hideBanner = () => {
		mutationBanner.isShown = false;
		clearTimer();
	};
	useEffect(() => {
		return () => clearTimer();
	}, [clearTimer]);
	const BannerComponent = ({ className }: { className?: string }) =>
		mutationBanner.isShown ? (
			<Banner
				title="title"
				hideTitle
				description={mutationBanner.description}
				variant={mutationBanner.variant}
				className={className || "m-2"}
				onDismiss={hideBanner}
			/>
		) : null;

	return { BannerComponent, showBanner, showErrorBanner, hideBanner };
};
