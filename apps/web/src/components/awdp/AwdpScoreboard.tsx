import { PeopleIcon, PackageIcon } from "@primer/octicons-react";
import { SegmentedControl } from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";
import { useState } from "react";

import type {
	AwdpScoreboardDetail,
	AwdpScoreboardGameBox,
	AwdpScoreboardRow,
} from "@/api/awdp";

/**
 * AWDP 赛事 Scoreboard（选手端）。
 *
 * SegmentedControl 双视图：
 *  - User view：聚合排名表（Rank / 参与者 / Break / Fix / Total）
 *  - Gamebox view：题目视角——每题显示 Break/Fix 成功主体数
 *    （Individual 显示"人数"、Team 显示"队伍数"；Fix 成功 = 至少一轮官方
 *    check PATCHED，不计成功几轮）。
 *
 * 数据来自 `GET /api/events/{id}/awdp/scoreboard`（后端一次聚合），
 * 纯展示组件，无内部 API/轮询。
 */

function ParticipantCell({ row }: { row: AwdpScoreboardRow }) {
	return (
		<div className="flex items-center gap-2">
			<div
				className="flex items-center justify-center rounded-full bg-gray-200 text-gray-500 font-medium shrink-0"
				style={{ width: 24, height: 24, fontSize: 10 }}
			>
				{row.subject_name?.[0]?.toUpperCase() || "?"}
			</div>
			<span className={row.is_me ? "font-semibold" : undefined}>
				{row.subject_name}
			</span>
			{row.is_me && (
				<span className="text-[11px] font-semibold uppercase tracking-wide text-[var(--fgColor-accent)]">
					me
				</span>
			)}
		</div>
	);
}

// ────────────────────────────────────────────────────────────────────────────
// User view：聚合排名表
// ────────────────────────────────────────────────────────────────────────────

function SummaryTable({ rows }: { rows: AwdpScoreboardRow[] }) {
	const columns = [
		{ accessorKey: "rank", header: "Rank", field: "rank" },
		{
			accessorKey: "subject_name",
			header: "Participant",
			field: "subject_name",
			rowHeader: true,
			renderCell: (row: AwdpScoreboardRow) => <ParticipantCell row={row} />,
		},
		{
			accessorKey: "break_score",
			header: "Break",
			field: "break_score",
			renderCell: (row: AwdpScoreboardRow) => (
				<span className="tabular-nums">{row.break_score}</span>
			),
		},
		{
			accessorKey: "fix_score",
			header: "Fix",
			field: "fix_score",
			renderCell: (row: AwdpScoreboardRow) => (
				<span className="tabular-nums">{row.fix_score}</span>
			),
		},
		{
			accessorKey: "total_score",
			header: "Total",
			field: "total_score",
			renderCell: (row: AwdpScoreboardRow) => (
				<strong className="tabular-nums">{row.total_score}</strong>
			),
		},
	];

	const table = useReactTable({
		data: rows,
		columns,
		getCoreRowModel: getCoreRowModel(),
	});

	return (
		<Table.Container>
			<Table.Subtitle id="awdp-scoreboard-subtitle">
				<div className="flex gap-2">
					<span>Break: 攻破他人 GameBox 获得</span>
					<span>Fix: 修复成功（官方 check PATCHED）获得</span>
					<span>Total: 总分</span>
				</div>
			</Table.Subtitle>
			<DataTable
				aria-labelledby="awdp-scoreboard"
				// @ts-ignore
				columns={columns}
				data={table
					.getRowModel()
					.rows.map((row) => ({ ...row.original, id: row.original.subject_id }))}
			/>
		</Table.Container>
	);
}

// ────────────────────────────────────────────────────────────────────────────
// Gamebox view：题目视角——Break/Fix 成功队伍数
// ────────────────────────────────────────────────────────────────────────────

type GameboxCountRow = {
	id: string;
	name: string;
	category: string;
	/** 攻破该题的主体数（提交过 flag 得分）。 */
	breakSubjects: number;
	/** 至少一轮官方 check PATCHED 的主体数（不计成功轮数）。 */
	fixSubjects: number;
};

function buildGameboxCounts(data: AwdpScoreboardDetail): GameboxCountRow[] {
	const { gameboxes, rows } = data;
	return gameboxes.map((gb: AwdpScoreboardGameBox, gi: number) => ({
		id: gb.id,
		name: gb.name,
		category: gb.category,
		breakSubjects: rows.filter((r) => r.break_status[gi]).length,
		fixSubjects: rows.filter((r) =>
			(r.fix_round_status[gi] ?? []).some((s) => s === "patched"),
		).length,
	}));
}

function GameboxTable({ data }: { data: AwdpScoreboardDetail }) {
	// Individual → 人数；Team → 队伍数（用户要求按赛制区分）。
	const unit = data.participant_mode === "team" ? "队伍" : "人";
	const rows = buildGameboxCounts(data);

	const columns = [
		{
			accessorKey: "name",
			header: "Gamebox",
			field: "name",
			rowHeader: true,
			renderCell: (row: GameboxCountRow) => (
				<div className="flex items-center gap-2">
					<span className="font-medium">{row.name}</span>
					<span className="text-xs text-[var(--fgColor-muted)]">
						{row.category}
					</span>
				</div>
			),
		},
		{
			accessorKey: "breakSubjects",
			header: `Break 成功${unit}数`,
			field: "breakSubjects",
			renderCell: (row: GameboxCountRow) => (
				<span className="tabular-nums">{row.breakSubjects}</span>
			),
		},
		{
			accessorKey: "fixSubjects",
			header: `Fix 成功${unit}数`,
			field: "fixSubjects",
			renderCell: (row: GameboxCountRow) => (
				<span className="tabular-nums">{row.fixSubjects}</span>
			),
		},
	];

	const table = useReactTable({
		data: rows,
		columns,
		getCoreRowModel: getCoreRowModel(),
	});

	return (
		<Table.Container>
			<Table.Subtitle id="awdp-gamebox-view-subtitle">
				<div className="flex gap-2">
					<span>Break: 攻破该题的{unit}数</span>
					<span>Fix: 至少一轮官方 check PATCHED 的{unit}数（不计成功轮数）</span>
				</div>
			</Table.Subtitle>
			<DataTable
				aria-labelledby="awdp-gamebox-view"
				// @ts-ignore
				columns={columns}
				data={table.getRowModel().rows.map((row) => row.original)}
			/>
		</Table.Container>
	);
}

// ────────────────────────────────────────────────────────────────────────────
// 入口：SegmentedControl 双视图
// ────────────────────────────────────────────────────────────────────────────

export function AwdpScoreboardView({
	data,
	className,
}: {
	data: AwdpScoreboardDetail;
	className?: string;
}) {
	const { gameboxes, rows } = data;
	const [view, setView] = useState<"user" | "gamebox">("user");

	if (gameboxes.length === 0 && rows.length === 0) {
		return (
			<div className={`${className ?? ""}`}>
				<p className="text-sm opacity-70">赛事开始后展示成绩。</p>
			</div>
		);
	}

	return (
		<div className={`flex flex-col gap-2 ${className ?? ""}`}>
			<SegmentedControl
				size="small"
				aria-label="Scoreboard view"
				onChange={(index) => setView(index === 0 ? "user" : "gamebox")}
			>
				<SegmentedControl.Button
					selected={view === "user"}
					leadingIcon={PeopleIcon}
				>
					User
				</SegmentedControl.Button>
				<SegmentedControl.Button
					selected={view === "gamebox"}
					leadingIcon={PackageIcon}
				>
					Gamebox
				</SegmentedControl.Button>
			</SegmentedControl>

			{view === "user" ? (
				<SummaryTable rows={rows} />
			) : gameboxes.length > 0 ? (
				<GameboxTable data={data} />
			) : (
				<p className="text-sm opacity-70">赛事开始后展示各题统计。</p>
			)}
		</div>
	);
}
