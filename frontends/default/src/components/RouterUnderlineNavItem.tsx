import { AppLink, type AppLinkProps } from "@/navigation";
import { UnderlineNav } from "@primer/react";
import { useMatchRoute } from "@tanstack/react-router";
import { forwardRef } from "react";

type PrimerAppLinkProps = AppLinkProps & { href?: string };

const PrimerAppLink = forwardRef<HTMLAnchorElement, PrimerAppLinkProps>(
	function PrimerAppLink({ href: _primerHref, ...props }, ref) {
		return <AppLink {...props} ref={ref} />;
	},
);

export type RouterUnderlineNavItemProps = {
	to: string;
	params?: Record<string, any>;
	children: React.ReactNode;
};

/**
 * TanStack Router + Primer UnderlineNav 的单锚点适配器。
 *
 * `UnderlineNav.Item` 本身就是链接元素，因此必须把 `AppLink` 作为 polymorphic
 * component 直接渲染；外层再包一层 `AppLink` 会生成 `<a><a>…</a></a>`，破坏
 * HTML、hydration 与 accessibility tree。
 */
export function RouterUnderlineNavItem({
	to,
	params,
	children,
}: RouterUnderlineNavItemProps) {
	const matchRoute = useMatchRoute();
	const isActive = matchRoute({ to, params, fuzzy: false });

	return (
		<UnderlineNav.Item
			as={PrimerAppLink}
			to={to}
			params={params}
			aria-current={isActive ? "page" : undefined}
		>
			{children}
		</UnderlineNav.Item>
	);
}
