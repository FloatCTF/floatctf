/**
 * 破坏性操作确认 —— 统一走 shadcn `AlertDialog`，**禁止**原生 `alert/confirm/prompt`。
 *
 * 用法：
 * ```tsx
 * const confirm = useConfirm();
 * const ok = await confirm({
 *   title: "删除赛事？",
 *   description: "删除后不可恢复。",
 *   consequences: ["所有实例容器会被回收", "参赛记录与积分一并删除"],
 *   tone: "danger",
 *   confirmText: "删除",
 *   confirmPhrase: "delete",           // 高危操作要求输入确认词
 * });
 * ```
 */

import {
	createContext,
	useCallback,
	useContext,
	useMemo,
	useRef,
	useState,
	type ReactNode,
} from "react";
import { AlertTriangle } from "lucide-react";

import {
	AlertDialog,
	AlertDialogAction,
	AlertDialogCancel,
	AlertDialogContent,
	AlertDialogDescription,
	AlertDialogFooter,
	AlertDialogHeader,
	AlertDialogTitle,
} from "~/components/ui/alert-dialog";
import { Input } from "~/components/ui/input";
import { Label } from "~/components/ui/label";
import { buttonVariants } from "~/components/ui/button";
import { cn } from "~/lib/utils";

export interface ConfirmOptions {
	title: string;
	description?: ReactNode;
	/** 明确列出「点下去会发生什么」—— 破坏性操作必须写真实后果。 */
	consequences?: string[];
	confirmText?: string;
	cancelText?: string;
	tone?: "default" | "danger";
	/** 高危操作：要求用户输入这段文字才能确认。 */
	confirmPhrase?: string;
}

type ConfirmFn = (options: ConfirmOptions) => Promise<boolean>;

interface PendingConfirm extends ConfirmOptions {
	resolve: (value: boolean) => void;
}

const ConfirmContext = createContext<ConfirmFn | null>(null);

export function ConfirmProvider({ children }: { children: ReactNode }) {
	const [pending, setPending] = useState<PendingConfirm | null>(null);
	const [phrase, setPhrase] = useState("");
	// 保证「关掉对话框（Esc / 遮罩）」也会 resolve(false)，不会悬挂 await。
	const settled = useRef(false);

	const confirm = useCallback<ConfirmFn>((options) => {
		settled.current = false;
		setPhrase("");
		return new Promise<boolean>((resolve) => {
			setPending({ ...options, resolve });
		});
	}, []);

	const finish = useCallback(
		(value: boolean) => {
			if (settled.current) return;
			settled.current = true;
			pending?.resolve(value);
			setPending(null);
			setPhrase("");
		},
		[pending],
	);

	const value = useMemo(() => confirm, [confirm]);
	const phraseOk = !pending?.confirmPhrase || phrase.trim() === pending.confirmPhrase;

	return (
		<ConfirmContext.Provider value={value}>
			{children}
			<AlertDialog
				open={pending !== null}
				onOpenChange={(open) => {
					if (!open) finish(false);
				}}
			>
				<AlertDialogContent>
					<AlertDialogHeader>
						<AlertDialogTitle className="flex items-center gap-2">
							{pending?.tone === "danger" ? (
								<AlertTriangle className="size-4 text-destructive" />
							) : null}
							{pending?.title}
						</AlertDialogTitle>
						{pending?.description ? (
							<AlertDialogDescription asChild>
								<div className="text-sm text-muted-foreground">{pending.description}</div>
							</AlertDialogDescription>
						) : null}
					</AlertDialogHeader>

					{pending?.consequences && pending.consequences.length > 0 ? (
						<ul className="list-disc space-y-1 rounded-md border border-destructive/30 bg-destructive/5 py-2 pr-3 pl-7 text-sm">
							{pending.consequences.map((item) => (
								<li key={item}>{item}</li>
							))}
						</ul>
					) : null}

					{pending?.confirmPhrase ? (
						<div className="space-y-1.5">
							<Label htmlFor="confirm-phrase">
								输入 <span className="font-mono">{pending.confirmPhrase}</span> 以确认
							</Label>
							<Input
								id="confirm-phrase"
								value={phrase}
								autoComplete="off"
								onChange={(event) => setPhrase(event.target.value)}
								placeholder={pending.confirmPhrase}
							/>
						</div>
					) : null}

					<AlertDialogFooter>
						<AlertDialogCancel onClick={() => finish(false)}>
							{pending?.cancelText ?? "取消"}
						</AlertDialogCancel>
						<AlertDialogAction
							disabled={!phraseOk}
							className={cn(
								pending?.tone === "danger" &&
									buttonVariants({ variant: "destructive" }),
							)}
							onClick={(event) => {
								event.preventDefault();
								if (!phraseOk) return;
								finish(true);
							}}
						>
							{pending?.confirmText ?? "确认"}
						</AlertDialogAction>
					</AlertDialogFooter>
				</AlertDialogContent>
			</AlertDialog>
		</ConfirmContext.Provider>
	);
}

export function useConfirm(): ConfirmFn {
	const context = useContext(ConfirmContext);
	if (!context) throw new Error("useConfirm 必须在 <ConfirmProvider> 内使用");
	return context;
}
