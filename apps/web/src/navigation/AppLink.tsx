import { Link } from "@tanstack/react-router";
import { forwardRef } from "react";
import { useNavigation } from "./NavigationContext";
import type { CoordinatorNavigateOptions } from "./navigation-types";

/**
 * 基于 TanStack Router Link 的类型安全 SPA 链接封装。
 *
 * 拦截普通左键单击，经导航协调器（预加载 → 提交）。
 * 以下情况绕过协调器，走浏览器默认行为：
 * - 修饰键点击（Ctrl/Cmd/Shift/Alt）
 * - 非左键（中键/右键）
 * - target != "_self"
 * - download 属性
 * - 外部 `to`（含 ":"，如 https://、mailto:、tel:）
 * - reloadDocument
 *
 * 保留：
 * - TanStack Router 类型化导航（to/params/search/hash）
 * - defaultPreload="intent"（悬停/触摸意图预加载）
 * - Link 激活态（activeProps/inactiveProps）
 */
export interface AppLinkProps
	extends Omit<
		React.AnchorHTMLAttributes<HTMLAnchorElement>,
		"href" | "children"
	> {
	/** 目标路由路径。外链（https://…、mailto:…）绕过 SPA。 */
	to: string;
	params?: Record<string, any>;
	search?: Record<string, any>;
	hash?: string;
	preload?: false | "intent" | "viewport" | "render";
	/** 外部 href——渲染为普通 a，绕过协调器。 */
	href?: string;
	/** 强制整页刷新——绕过协调器。 */
	reloadDocument?: boolean;
	children?: React.ReactNode;
}

/** 判断本次点击是否应绕过协调器。 */
function shouldBypass(
	event: React.MouseEvent<HTMLAnchorElement>,
	props: AppLinkProps,
): boolean {
	if (event.button !== 0) return true; // non-left click
	if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey)
		return true;
	if (event.defaultPrevented) return true;
	if (props.target && props.target !== "_self") return true;
	if (props.download !== undefined) return true;
	if (props.reloadDocument) return true;
	// 外链 / mailto: / tel: → TanStack Link 视为外部链接
	if (typeof props.to === "string" && props.to.includes(":")) return true;
	return false;
}

export const AppLink = forwardRef<HTMLAnchorElement, AppLinkProps>(
	function AppLink(
		{
			onClick,
			children,
			target,
			download,
			href,
			reloadDocument,
			style,
			className,
			"aria-current": ariaCurrent,
			preload,
			...rest
		},
		ref,
	) {
		const { navigateWithTransition } = useNavigation();
		const { to, params, search, hash, ...anchorProps } = rest;

		// download / 显式外链 → 普通 a 标签，不做 SPA 导航。
		if (download !== undefined || href !== undefined) {
			return (
				<a
					{...anchorProps}
					ref={ref}
					href={href ?? to}
					download={
						typeof download === "string" ? download : download ? "" : undefined
					}
					target={target}
					style={style}
					className={className}
					aria-current={ariaCurrent}
					onClick={onClick}
				>
					{children}
				</a>
			);
		}

		const handleClick = (event: React.MouseEvent<HTMLAnchorElement>) => {
			if (
				shouldBypass(event, {
					...anchorProps,
					to,
					params,
					search,
					hash,
					target,
					reloadDocument,
				})
			) {
				onClick?.(event);
				return;
			}

			// 拦截：阻止浏览器默认导航。
			event.preventDefault();
			onClick?.(event);

			const navOpts: CoordinatorNavigateOptions = {
				to,
				params,
				search,
				hash,
				preload,
				target,
				reloadDocument,
			};

			void navigateWithTransition(navOpts);
		};

		return (
			<Link
				{...anchorProps}
				ref={ref}
				to={to}
				params={params as never}
				search={search as never}
				hash={hash}
				target={target}
				reloadDocument={reloadDocument}
				preload={preload}
				style={style}
				className={className}
				aria-current={ariaCurrent}
				onClick={handleClick}
			>
				{children}
			</Link>
		);
	},
);
