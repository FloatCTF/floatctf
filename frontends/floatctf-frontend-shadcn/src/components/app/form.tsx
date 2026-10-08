/**
 * 表单基元 —— 与 shadcn `Form`（react-hook-form）互补：
 * 这里提供**不依赖表单库**的通用容器（Field / FormSheet / FormDialog / FormFooter），
 * 让简单 CRUD 与复杂表单共享同一套版式。使用 react-hook-form 时可直接把 `FormItem` 放进 `Field`。
 */

import type { ReactNode } from "react";
import { Loader2 } from "lucide-react";

import { Button } from "~/components/ui/button";
import { Label } from "~/components/ui/label";
import {
	Sheet,
	SheetContent,
	SheetDescription,
	SheetFooter,
	SheetHeader,
	SheetTitle,
} from "~/components/ui/sheet";
import { cn } from "~/lib/utils";

export function Field({
	label,
	htmlFor,
	hint,
	error,
	required,
	children,
	className,
	actions,
}: {
	label?: ReactNode;
	htmlFor?: string;
	hint?: ReactNode;
	error?: unknown;
	required?: boolean;
	children: ReactNode;
	className?: string;
	actions?: ReactNode;
}) {
	return (
		<div className={cn("space-y-1.5", className)}>
			{label ? (
				<div className="flex items-center justify-between gap-2">
					<Label htmlFor={htmlFor} className="text-xs">
						{label}
						{required ? <span className="ml-0.5 text-destructive">*</span> : null}
					</Label>
					{actions}
				</div>
			) : null}
			{children}
			{hint && !error ? <p className="text-xs text-muted-foreground">{hint}</p> : null}
			{error ? <FieldError error={error} /> : null}
		</div>
	);
}

/** 表单错误：字符串数组（zod flatten）与普通字符串都支持。 */
export function FieldError({ error }: { error: unknown }) {
	if (!error) return null;
	const messages = Array.isArray(error)
		? error.map((item) => String(item))
		: [typeof error === "string" ? error : String((error as { message?: string })?.message ?? error)];
	return (
		<ul className="space-y-0.5">
			{messages.map((message) => (
				<li key={message} className="text-xs text-destructive">
					{message}
				</li>
			))}
		</ul>
	);
}

export function FormGrid({
	children,
	columns = 2,
	className,
}: {
	children: ReactNode;
	columns?: 1 | 2 | 3;
	className?: string;
}) {
	return (
		<div
			className={cn(
				"grid gap-4",
				columns === 1 ? "grid-cols-1" : columns === 3 ? "sm:grid-cols-3" : "sm:grid-cols-2",
				className,
			)}
		>
			{children}
		</div>
	);
}

export function FormSheet({
	open,
	onOpenChange,
	title,
	description,
	children,
	footer,
	width = "md",
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	title: ReactNode;
	description?: ReactNode;
	children: ReactNode;
	footer?: ReactNode;
	/** md = 更窄（默认 28rem+）；lg / xl 用于字段较多的表单。 */
	width?: "md" | "lg" | "xl";
}) {
	const widthClass =
		width === "xl"
			? "sm:max-w-3xl"
			: width === "lg"
				? "sm:max-w-2xl"
				: "sm:max-w-xl";
	return (
		<Sheet open={open} onOpenChange={onOpenChange}>
			<SheetContent className={cn("flex w-full flex-col gap-0 overflow-y-auto", widthClass)}>
				<SheetHeader>
					<SheetTitle>{title}</SheetTitle>
					{description ? <SheetDescription>{description}</SheetDescription> : null}
				</SheetHeader>
				<div className="flex-1 space-y-4 px-4 pb-4">{children}</div>
				{footer ? <SheetFooter>{footer}</SheetFooter> : null}
			</SheetContent>
		</Sheet>
	);
}

export function FormFooter({
	onCancel,
	submitLabel = "保存",
	cancelLabel = "取消",
	isPending = false,
	disabled = false,
	formId,
	hint,
}: {
	onCancel?: () => void;
	submitLabel?: string;
	cancelLabel?: string;
	isPending?: boolean;
	disabled?: boolean;
	/** 表单 id：放在 Sheet 底部仍能提交 `<form id=...>`。 */
	formId?: string;
	hint?: ReactNode;
}) {
	return (
		<div className="flex w-full flex-wrap items-center justify-between gap-2">
			<span className="text-xs text-muted-foreground">{hint}</span>
			<div className="flex items-center gap-2">
				{onCancel ? (
					<Button type="button" variant="outline" onClick={onCancel} disabled={isPending}>
						{cancelLabel}
					</Button>
				) : null}
				<Button type="submit" form={formId} disabled={isPending || disabled}>
					{isPending ? <Loader2 className="size-4 animate-spin" /> : null}
					{submitLabel}
				</Button>
			</div>
		</div>
	);
}

export function SubmitButton({
	isPending,
	children,
	...props
}: React.ComponentProps<typeof Button> & { isPending?: boolean }) {
	return (
		<Button type="submit" disabled={isPending || props.disabled} {...props}>
			{isPending ? <Loader2 className="size-4 animate-spin" /> : null}
			{children}
		</Button>
	);
}
