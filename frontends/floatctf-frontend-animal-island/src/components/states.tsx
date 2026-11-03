/**
 * 三态（loading / empty / error）统一组件。
 *
 * 存在的理由：三态**必须**由同一个组件表达，否则每个页面会长出自己的一套措辞与
 * 体量，切换时布局跳动、错误被弱化成一行灰字。这里刻意让三者共享同一个容器高度。
 *
 * 视觉规则：状态图标只能用 `naive-icons`（禁止 emoji / Unicode 符号 / 手写 SVG），
 * 颜色一律走 `var(--animal-*)`；可点击处优先用库的 `Button`，不写原生按钮。
 */
import type { ReactNode } from "react";
import { Button, Card, Loading, Skeleton } from "animal-island-ui";
import { AlertTriangleIcon, InboxIcon, RefreshIcon } from "naive-icons";

import { errorText } from "~/api/errors";

export function LoadingState({ label = "加载中…" }: { label?: string }) {
	return (
		<div className="ai-state" role="status" aria-live="polite">
			<Loading active tip={label} />
		</div>
	);
}

/** 骨架屏：用于列表/卡片区，比整屏 Loading 更少跳动。 */
export function SkeletonCards({ count = 6 }: { count?: number }) {
	return (
		<div className="ai-grid">
			{Array.from({ length: count }, (_, i) => (
				<Card key={i}>
					<Skeleton variant="paragraph" rows={3} />
				</Card>
			))}
		</div>
	);
}

export function EmptyState({
	title = "这里还空着",
	description,
	action,
}: {
	title?: string;
	description?: ReactNode;
	action?: ReactNode;
}) {
	return (
		<div className="ai-state">
			<div className="ai-state-icon">
				<InboxIcon size={28} />
			</div>
			<div className="ai-state-title">{title}</div>
			{description ? <div className="ai-muted">{description}</div> : null}
			{action}
		</div>
	);
}

export function ErrorState({
	error,
	onRetry,
}: {
	error: unknown;
	onRetry?: () => void;
}) {
	return (
		<div className="ai-state">
			<div
				className="ai-state-icon"
				style={{
					background: "#fdeceb",
					borderColor: "var(--animal-error-color)",
					color: "var(--animal-error-color)",
				}}
			>
				<AlertTriangleIcon size={28} />
			</div>
			<div className="ai-state-title">出错了</div>
			<div className="ai-muted">{errorText(error)}</div>
			{onRetry ? (
				<Button
					type="default"
					size="small"
					icon={<RefreshIcon size={16} />}
					onClick={onRetry}
				>
					重试
				</Button>
			) : null}
		</div>
	);
}
