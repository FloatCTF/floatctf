import { Banner } from "@primer/react/experimental";
import { useReactive } from "ahooks";
import { useCallback, useEffect, useRef } from "react";

import { formatApiError } from "./apiErrorMessage";

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

	/** `overrides` 可按状态码覆盖文案（例如登录页把 401 解释为账号密码错误）。 */
	const showErrorBanner = (
		error: unknown,
		overrides?: Record<number, string>,
	) => {
		showBanner("critical", formatApiError(error, overrides));
	};

	const hideBanner = () => {
		mutationBanner.isShown = false;
		clearTimer();
	};
	useEffect(() => {
		return () => clearTimer();
	}, [clearTimer]);
	const bannerTitle =
		mutationBanner.variant === "success"
			? "操作成功"
			: mutationBanner.variant === "critical"
				? "操作失败"
				: "提示";

	const BannerComponent = ({ className }: { className?: string }) =>
		mutationBanner.isShown ? (
			<Banner
				title={bannerTitle}
				hideTitle
				description={mutationBanner.description}
				variant={mutationBanner.variant}
				className={className || "m-2"}
				onDismiss={hideBanner}
			/>
		) : null;

	return { BannerComponent, showBanner, showErrorBanner, hideBanner };
};
