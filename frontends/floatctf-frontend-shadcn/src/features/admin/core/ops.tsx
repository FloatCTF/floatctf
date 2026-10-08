/**
 * 题库 / GameBox 库共用的「一次性操作结果」表格 —— 扫描、校验、构建。
 *
 * 铁律：后端返回的**每一条**结果都要如实展示（含 `message` 里的失败原因），
 * 不允许用一句「操作成功」把失败吞掉。
 */

import type { ReactNode } from "react";

import { TonePill, type PillTone } from "~/components/app/badges";
import { DataTable, type DataTableColumn } from "~/components/app/data-table";
import { MonoText } from "~/components/app/page";

/** `ChallengeScanItem` / `GameBoxScanItem` 的结构（两者字段完全一致）。 */
export interface ScanRow {
	safe_name: string;
	name: string | null;
	version: string | null;
	status: "added" | "skipped" | "error";
	message: string;
}

const SCAN_STATUS: Record<ScanRow["status"], { tone: PillTone; label: string }> = {
	added: { tone: "success", label: "已登记" },
	skipped: { tone: "muted", label: "已跳过" },
	error: { tone: "danger", label: "失败" },
};

export function ScanResultTable({ items }: { items: ScanRow[] }): ReactNode {
	const columns: DataTableColumn<ScanRow>[] = [
		{
			id: "safe_name",
			header: "safe_name",
			cell: (row) => <MonoText>{row.safe_name}</MonoText>,
			sortValue: (row) => row.safe_name,
		},
		{
			id: "name",
			header: "名称",
			cell: (row) =>
				row.name ? (
					<span className="text-sm">{row.name}</span>
				) : (
					<span className="text-xs text-muted-foreground">—</span>
				),
			sortValue: (row) => row.name ?? "",
			hideBelow: "sm",
		},
		{
			id: "version",
			header: "版本",
			cell: (row) =>
				row.version ? (
					<MonoText>{row.version}</MonoText>
				) : (
					<span className="text-xs text-muted-foreground">—</span>
				),
			hideBelow: "md",
		},
		{
			id: "status",
			header: "状态",
			cell: (row) => {
				const entry = SCAN_STATUS[row.status] ?? { tone: "muted" as PillTone, label: row.status };
				return (
					<TonePill tone={entry.tone} title={row.status}>
						{entry.label}
					</TonePill>
				);
			},
			sortValue: (row) => row.status,
		},
		{
			id: "message",
			header: "说明",
			cell: (row) => <span className="text-xs text-muted-foreground">{row.message || "—"}</span>,
		},
	];

	return (
		<DataTable
			data={items}
			getRowId={(row) => row.safe_name}
			columns={columns}
			mobileCard={(row) => (
				<div className="space-y-1.5">
					<div className="flex items-center justify-between gap-2">
						<MonoText>{row.safe_name}</MonoText>
						<TonePill tone={(SCAN_STATUS[row.status] ?? { tone: "muted" as PillTone }).tone}>
							{SCAN_STATUS[row.status]?.label ?? row.status}
						</TonePill>
					</div>
					<p className="text-sm">{row.name ?? "—"}</p>
					<p className="text-xs text-muted-foreground">{row.message || "—"}</p>
				</div>
			)}
			empty={<span>扫描未返回任何条目</span>}
		/>
	);
}

/** `BuildChallengeResult` / `GameBoxBuildResult` 的结构。 */
export interface BuildRow {
	is_ok: boolean;
	message: string;
}

export function BuildResultTable<T extends BuildRow>({
	items,
	nameOf,
	label,
}: {
	items: T[];
	nameOf: (row: T) => string;
	label: string;
}): ReactNode {
	const columns: DataTableColumn<T>[] = [
		{
			id: "name",
			header: label,
			cell: (row) => <span className="text-sm font-medium">{nameOf(row)}</span>,
			sortValue: (row) => nameOf(row),
		},
		{
			id: "is_ok",
			header: "结果",
			cell: (row) => (
				<TonePill tone={row.is_ok ? "success" : "danger"}>{row.is_ok ? "成功" : "失败"}</TonePill>
			),
			sortValue: (row) => (row.is_ok ? 1 : 0),
		},
		{
			id: "message",
			header: "后端消息",
			cell: (row) => (
				<span className={row.is_ok ? "text-xs text-muted-foreground" : "text-xs text-destructive"}>
					{row.message || "—"}
				</span>
			),
		},
	];

	return (
		<DataTable
			data={items}
			getRowId={(row) => nameOf(row)}
			columns={columns}
			mobileCard={(row) => (
				<div className="space-y-1.5">
					<div className="flex items-center justify-between gap-2">
						<span className="text-sm font-medium">{nameOf(row)}</span>
						<TonePill tone={row.is_ok ? "success" : "danger"}>
							{row.is_ok ? "成功" : "失败"}
						</TonePill>
					</div>
					<p className="text-xs text-muted-foreground">{row.message || "—"}</p>
				</div>
			)}
			empty={<span>没有可执行的对象（后端未返回任何结果）</span>}
		/>
	);
}
