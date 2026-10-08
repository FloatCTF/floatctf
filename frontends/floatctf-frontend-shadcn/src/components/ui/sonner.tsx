import {
	CircleCheckIcon,
	InfoIcon,
	Loader2Icon,
	OctagonXIcon,
	TriangleAlertIcon,
} from "lucide-react";
import { Toaster as Sonner, type ToasterProps } from "sonner";

import { useResolvedTheme } from "~/lib/theme";

/**
 * 全局 toast 宿主（shadcn `sonner`）。
 * 主题取自本前端自己的主题 store（不引入 next-themes —— 本前端不是 Next 应用）。
 */
function Toaster({ ...props }: ToasterProps) {
	const theme = useResolvedTheme();

	return (
		<Sonner
			theme={theme}
			className="toaster group"
			position="bottom-right"
			richColors
			closeButton
			icons={{
				success: <CircleCheckIcon className="size-4" />,
				info: <InfoIcon className="size-4" />,
				warning: <TriangleAlertIcon className="size-4" />,
				error: <OctagonXIcon className="size-4" />,
				loading: <Loader2Icon className="size-4 animate-spin" />,
			}}
			style={
				{
					"--normal-bg": "var(--popover)",
					"--normal-text": "var(--popover-foreground)",
					"--normal-border": "var(--border)",
					"--border-radius": "var(--radius)",
				} as React.CSSProperties
			}
			{...props}
		/>
	);
}

export { Toaster };
