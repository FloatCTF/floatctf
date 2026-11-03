/**
 * 统一确认弹窗。
 *
 * 为什么必须有：破坏性操作（删除、重置环境、下线赛事）如果各自用裸 `Modal`，
 * 文案与按钮语义很快就不一致 —— 而这个平台上一次误点重置会毁掉整场比赛。
 * 因此确认走统一入口，并强制要求调用方写清**真实后果**。
 */
import { createContext, useCallback, useContext, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Button, Input, Modal } from "animal-island-ui";

export interface ConfirmOptions {
	title: string;
	/** 必须写清"会发生什么"，而不是"确定吗？"。 */
	consequence: ReactNode;
	okText?: string;
	cancelText?: string;
	danger?: boolean;
	/** 高危操作：要求用户原样输入该字符串才能确认。 */
	confirmPhrase?: string;
}

type ConfirmFn = (options: ConfirmOptions) => Promise<boolean>;

const ConfirmContext = createContext<ConfirmFn | null>(null);

export function useConfirm(): ConfirmFn {
	const fn = useContext(ConfirmContext);
	if (!fn) throw new Error("useConfirm 必须在 ConfirmProvider 内使用");
	return fn;
}

export function ConfirmProvider({ children }: { children: ReactNode }) {
	const [options, setOptions] = useState<ConfirmOptions | null>(null);
	const [typed, setTyped] = useState("");
	const [busy, setBusy] = useState(false);
	const resolver = useRef<((ok: boolean) => void) | null>(null);

	const confirm = useCallback<ConfirmFn>((opts) => {
		setOptions(opts);
		setTyped("");
		setBusy(false);
		return new Promise<boolean>((resolve) => {
			resolver.current = resolve;
		});
	}, []);

	const finish = useCallback((ok: boolean) => {
		resolver.current?.(ok);
		resolver.current = null;
		setOptions(null);
	}, []);

	const phraseOk = useMemo(() => {
		if (!options?.confirmPhrase) return true;
		return typed.trim() === options.confirmPhrase;
	}, [options, typed]);

	return (
		<ConfirmContext.Provider value={confirm}>
			{children}
			<Modal
				open={options !== null}
				title={options?.title ?? ""}
				onClose={() => finish(false)}
				// 数据表单场景关掉打字机：确认弹窗的重点是让人**读清后果**，
				// 逐字动画反而拖延了本来就需要严肃对待的一步。
				typewriter={false}
				footer={
					<>
						<Button onClick={() => finish(false)} loading={busy}>
							{options?.cancelText ?? "取消"}
						</Button>
						<Button
							type="primary"
							danger={options?.danger}
							disabled={!phraseOk}
							loading={busy}
							onClick={() => {
								setBusy(true);
								finish(true);
							}}
						>
							{options?.okText ?? "确定"}
						</Button>
					</>
				}
			>
				<div className="ai-col" style={{ gap: 12 }}>
					<div>{options?.consequence}</div>
					{options?.confirmPhrase ? (
						<div className="ai-col" style={{ gap: 6 }}>
							<span className="ai-muted">
								这是高危操作。请输入 <span className="ai-mono">{options.confirmPhrase}</span> 以确认：
							</span>
							<Input
								value={typed}
								placeholder={options.confirmPhrase}
								onChange={(e) => setTyped(e.target.value)}
							/>
						</div>
					) : null}
				</div>
			</Modal>
		</ConfirmContext.Provider>
	);
}
