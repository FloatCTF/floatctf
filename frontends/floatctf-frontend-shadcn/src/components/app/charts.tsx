/**
 * 趋势图（自研 SVG，避免为两个图引入重量级图表库）。
 * 只做「真实数据 → 可视化」，不参与任何状态判定。
 */

import { useMemo, useState, type ReactNode } from "react";

import { cn } from "~/lib/utils";

export interface TrendPoint {
	/** 时间戳（毫秒）或任意数值 x。 */
	x: number;
	y: number;
}

export interface TrendSeries {
	name: string;
	points: TrendPoint[];
	color?: string;
}

const DEFAULT_COLORS = [
	"var(--chart-1)",
	"var(--chart-2)",
	"var(--chart-3)",
	"var(--chart-4)",
	"var(--chart-5)",
];

export function TrendChart({
	series,
	height = 260,
	formatY = (value: number) => String(Math.round(value)),
	formatX,
	className,
	emptyText = "暂无趋势数据",
}: {
	series: TrendSeries[];
	height?: number;
	formatY?: (value: number) => string;
	formatX?: (value: number) => string;
	className?: string;
	emptyText?: string;
}): ReactNode {
	const [hoverX, setHoverX] = useState<number | null>(null);

	const { minX, maxX, minY, maxY } = useMemo(() => {
		const xs: number[] = [];
		const ys: number[] = [];
		for (const item of series) {
			for (const point of item.points) {
				xs.push(point.x);
				ys.push(point.y);
			}
		}
		if (xs.length === 0) return { minX: 0, maxX: 1, minY: 0, maxY: 1 };
		return {
			minX: Math.min(...xs),
			maxX: Math.max(...xs),
			minY: Math.min(0, ...ys),
			maxY: Math.max(...ys, 1),
		};
	}, [series]);

	const hasData = series.some((item) => item.points.length > 0);
	if (!hasData) {
		return (
			<div
				className={cn(
					"flex items-center justify-center rounded-md border border-dashed text-sm text-muted-foreground",
					className,
				)}
				style={{ height }}
			>
				{emptyText}
			</div>
		);
	}

	const width = 800;
	const padding = { top: 12, right: 16, bottom: 26, left: 52 };
	const innerWidth = width - padding.left - padding.right;
	const innerHeight = height - padding.top - padding.bottom;

	const scaleX = (value: number) =>
		padding.left + (maxX === minX ? 0 : ((value - minX) / (maxX - minX)) * innerWidth);
	const scaleY = (value: number) =>
		padding.top + innerHeight - ((value - minY) / (maxY - minY || 1)) * innerHeight;

	const yTicks = [0, 0.25, 0.5, 0.75, 1].map((ratio) => minY + ratio * (maxY - minY));
	const xTickValues = [minX, minX + (maxX - minX) / 2, maxX];

	const hoveredEntries =
		hoverX === null
			? []
			: series
					.map((item, index) => {
						let nearest: TrendPoint | null = null;
						for (const point of item.points) {
							if (nearest === null || Math.abs(point.x - hoverX) < Math.abs(nearest.x - hoverX)) {
								nearest = point;
							}
						}
						return nearest
							? {
									name: item.name,
									color: item.color ?? DEFAULT_COLORS[index % DEFAULT_COLORS.length],
									point: nearest,
								}
							: null;
					})
					.filter((entry): entry is NonNullable<typeof entry> => entry !== null);

	return (
		<div className={cn("w-full", className)}>
			<svg
				viewBox={`0 0 ${width} ${height}`}
				className="w-full"
				style={{ height }}
				role="img"
				aria-label="趋势图"
				onMouseLeave={() => setHoverX(null)}
				onMouseMove={(event) => {
					const rect = event.currentTarget.getBoundingClientRect();
					const ratio = (event.clientX - rect.left) / rect.width;
					const x = minX + ratio * (maxX - minX);
					setHoverX(x);
				}}
			>
				{yTicks.map((tick) => (
					<g key={`y-${tick}`}>
						<line
							x1={padding.left}
							x2={width - padding.right}
							y1={scaleY(tick)}
							y2={scaleY(tick)}
							stroke="var(--border)"
							strokeDasharray="3 3"
						/>
						<text
							x={padding.left - 8}
							y={scaleY(tick) + 4}
							textAnchor="end"
							className="fill-muted-foreground"
							style={{ fontSize: 11 }}
						>
							{formatY(tick)}
						</text>
					</g>
				))}

				{xTickValues.map((tick) => (
					<text
						key={`x-${tick}`}
						x={scaleX(tick)}
						y={height - 8}
						textAnchor="middle"
						className="fill-muted-foreground"
						style={{ fontSize: 11 }}
					>
						{formatX ? formatX(tick) : new Date(tick).toLocaleTimeString("zh-CN", { hour12: false })}
					</text>
				))}

				{series.map((item, index) => {
					const color = item.color ?? DEFAULT_COLORS[index % DEFAULT_COLORS.length];
					const sorted = [...item.points].sort((a, b) => a.x - b.x);
					const path = sorted
						.map((point, pointIndex) => `${pointIndex === 0 ? "M" : "L"} ${scaleX(point.x)} ${scaleY(point.y)}`)
						.join(" ");
					return (
						<path
							key={item.name}
							d={path}
							fill="none"
							stroke={color}
							strokeWidth={2}
							strokeLinejoin="round"
							strokeLinecap="round"
						/>
					);
				})}

				{hoverX !== null ? (
					<line
						x1={scaleX(hoverX)}
						x2={scaleX(hoverX)}
						y1={padding.top}
						y2={padding.top + innerHeight}
						stroke="var(--muted-foreground)"
						strokeDasharray="2 2"
					/>
				) : null}

				{hoveredEntries.map((entry) => (
					<circle
						key={entry.name}
						cx={scaleX(entry.point.x)}
						cy={scaleY(entry.point.y)}
						r={3.5}
						fill={entry.color}
					/>
				))}
			</svg>

			<div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
				{series.map((item, index) => (
					<span key={item.name} className="inline-flex items-center gap-1.5">
						<span
							className="inline-block size-2 rounded-full"
							style={{ background: item.color ?? DEFAULT_COLORS[index % DEFAULT_COLORS.length] }}
						/>
						{item.name}
						{hoverX !== null
							? (() => {
									const entry = hoveredEntries.find((candidate) => candidate.name === item.name);
									return entry ? (
										<span className="tnum text-foreground">{formatY(entry.point.y)}</span>
									) : null;
								})()
							: null}
					</span>
				))}
			</div>
		</div>
	);
}

/** 迷你走势（表格单元 / 卡片角落用）。 */
export function Sparkline({
	points,
	width = 120,
	height = 28,
	className,
}: {
	points: number[];
	width?: number;
	height?: number;
	className?: string;
}): ReactNode {
	if (points.length < 2) {
		return <span className="text-xs text-muted-foreground">—</span>;
	}
	const min = Math.min(...points);
	const max = Math.max(...points);
	const path = points
		.map((value, index) => {
			const x = (index / (points.length - 1)) * width;
			const y = height - ((value - min) / (max - min || 1)) * height;
			return `${index === 0 ? "M" : "L"} ${x.toFixed(1)} ${y.toFixed(1)}`;
		})
		.join(" ");
	return (
		<svg viewBox={`0 0 ${width} ${height}`} width={width} height={height} className={className}>
			<path d={path} fill="none" stroke="var(--chart-2)" strokeWidth={1.5} />
		</svg>
	);
}
