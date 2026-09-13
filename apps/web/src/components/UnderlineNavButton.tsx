import { UnderlineNav } from "@primer/react";
import { forwardRef } from "react";

type PrimerButtonAdapterProps = Omit<
	React.ButtonHTMLAttributes<HTMLButtonElement>,
	"type"
> & {
	// Primer UnderlineNav.Item 无论 `as` 是什么都会注入 href="#"；button 不需要它。
	href?: string;
	type?: "button" | "submit" | "reset";
};

const PrimerButtonAdapter = forwardRef<
	HTMLButtonElement,
	PrimerButtonAdapterProps
>(function PrimerButtonAdapter(
	{ href: _primerHref, type = "button", ...props },
	ref,
) {
	return <button {...props} ref={ref} type={type} />;
});

export type UnderlineNavButtonProps = {
	current?: boolean;
	children: React.ReactNode;
	onClick: () => void;
};

/**
 * 用于只切换本地视图、并不代表 URL 路由的 UnderlineNav 控件。
 *
 * Primer 的默认 Item 是 `<a href="#">`；把纯状态切换建模为 button 可以保留
 * UnderlineNav 外观，同时给浏览器、键盘和辅助技术正确的交互语义。
 */
export function UnderlineNavButton({
	current = false,
	children,
	onClick,
}: UnderlineNavButtonProps) {
	return (
		<UnderlineNav.Item
			as={PrimerButtonAdapter}
			aria-current={current ? "page" : undefined}
			onClick={onClick}
		>
			{children}
		</UnderlineNav.Item>
	);
}
