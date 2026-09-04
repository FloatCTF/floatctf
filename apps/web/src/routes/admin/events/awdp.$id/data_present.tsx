import {
	RocketIcon,
	ScreenFullIcon,
	ScreenNormalIcon,
	TriangleRightIcon,
} from "@primer/octicons-react";
import { Label, LabelGroup, Spinner, Text, Timeline } from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";
import { useState } from "react";

import { type AwdpDataPresent, awdpAdminApi } from "@/api/awdp";
import { RemainingTimer } from "@/routes/service/events/jeopardy.$id/route";
import { TrendChart } from "@/routes/service/events/jeopardy.$id/trend";
import { DatetimeToShow } from "@/util";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/awdp/$id/data_present")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

function RouteComponent() {
	const [isFull, setIsFull] = useState(false);
	const { id } = Route.useParams();
	const { data, isLoading, isError } = useQuery({
		queryKey: ["awdp-event-data-present", id],
		queryFn: () => awdpAdminApi.dataPresent(id),
		refetchInterval: 1000 * 30,
	});
	const dp: AwdpDataPresent | undefined = data?.data;

	if (isLoading) {
		return <Spinner size="large" />;
	}

	if (isError || !dp || !dp.event) {
		return <div>Error loading data</div>;
	}

	return (
		<div className="relative w-full h-full mb-2">
			<div
				className={`transition-all duration-500 ease-in-out
${
	isFull
		? "fixed top-0 left-0 w-screen h-screen bg-white z-[9999] scale-100 opacity-100 overflow-auto"
		: ""
}`}
			>
				<button
					type="button"
					onClick={() => setIsFull(!isFull)}
					className="absolute top-2 right-2 p-2 rounded hover:bg-gray-300"
				>
					{isFull ? (
						<ScreenNormalIcon size={24} />
					) : (
						<ScreenFullIcon size={24} />
					)}
				</button>

				<div className="w-full h-full p-3">
					<div id="head" className="flex gap-2 items-center">
						<RocketIcon size={20} />
						<h3>
							{dp.event.title} - {dp.event.family}/{dp.event.participant_mode}
						</h3>
						<LabelGroup>
							<Label variant="success" size="large">
								Users {dp.user_count}
							</Label>
							<Label variant="accent" size="large">
								Teams {dp.team_count}
							</Label>
							<Label variant="attention" size="large">
								GameBoxes {dp.gameboxes.length}
							</Label>
						</LabelGroup>
					</div>
					<RemainingTimer
						start_at={DatetimeToShow(dp.event.start_time)}
						end_at={DatetimeToShow(dp.event.end_time)}
					/>
					<div className="flex">
						<div id="top-box" className="flex flex-col flex-8">
							<div className="flex">
								<div className="flex items-start flex-4">
									<ScoreboardTop10 rows={dp.scoreboard_top10} />
								</div>
								<div className="flex items-center justify-center flex-9">
									<TrendChart data={dp.trend} className="w-full h-full" />
								</div>
							</div>
							<div id="bottom-box" className="flex-8">
								<GameBoxesView data={dp.gameboxes} />
							</div>
						</div>
						<div id="side-bar" className="flex-2 p-2">
							<h3>Recent Activity</h3>
							<Timeline>
								{dp.recent_activity.map((item, idx) => (
									<Timeline.Item key={`${item.created_at}-${idx}`}>
										<Timeline.Badge>
											<TriangleRightIcon />
										</Timeline.Badge>
										<Timeline.Body>
											<Text>
												<Text sx={{ fontWeight: 600 }}>
													{item.subject_name}
												</Text>{" "}
												<Text sx={{ color: "fg.muted" }}>
													{item.action === "break" ? "broke" : "fixed"}
												</Text>{" "}
												<Text
													sx={{
														fontFamily: "mono",
														fontWeight: 900,
														textDecoration: "underline",
													}}
												>
													{item.gamebox_category}/{item.gamebox_name}
												</Text>
												<br />
												<Text sx={{ color: "fg.muted" }}>
													{item.delta > 0 ? `+${item.delta}` : item.delta} @{" "}
													<span>{DatetimeToShow(item.created_at)}</span>
												</Text>
											</Text>
										</Timeline.Body>
									</Timeline.Item>
								))}
							</Timeline>
						</div>
					</div>
				</div>
			</div>
		</div>
	);
}

function ScoreboardTop10({
	rows,
}: { rows: AwdpDataPresent["scoreboard_top10"] }) {
	const columns = [
		{
			accessorKey: "rank",
			header: "Rank",
			field: "rank",
		},
		{
			accessorKey: "subject_name",
			header: "Participant",
			field: "subject_name",
			rowHeader: true,
			renderCell: (row: AwdpDataPresent["scoreboard_top10"][number]) => (
				<div className="flex items-center gap-2">
					<div
						className="flex items-center justify-center rounded-full bg-gray-200 text-gray-500 font-medium shrink-0"
						style={{ width: 24, height: 24, fontSize: 10 }}
					>
						{row.subject_name?.[0]?.toUpperCase() || "?"}
					</div>
					<span>{row.subject_name}</span>
				</div>
			),
		},
		{
			accessorKey: "break_score",
			header: "Break",
			field: "break_score",
			renderCell: (row: AwdpDataPresent["scoreboard_top10"][number]) => (
				<span className="tabular-nums">{row.break_score}</span>
			),
		},
		{
			accessorKey: "fix_score",
			header: "Fix",
			field: "fix_score",
			renderCell: (row: AwdpDataPresent["scoreboard_top10"][number]) => (
				<span className="tabular-nums">{row.fix_score}</span>
			),
		},
		{
			accessorKey: "total_score",
			header: "Total",
			field: "total_score",
			renderCell: (row: AwdpDataPresent["scoreboard_top10"][number]) => (
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
		<Table.Container className="mt-3 w-full">
			<Table.Subtitle id="awdp-data-top10-subtitle">
				<span>Top 10</span>
			</Table.Subtitle>
			<DataTable
				aria-labelledby="awdp-data-top10"
				// @ts-ignore
				columns={columns}
				data={table.getRowModel().rows.map((row) => ({
					...row.original,
					id: row.original.subject_id,
				}))}
			/>
		</Table.Container>
	);
}
function GameBoxTile({ item }: { item: AwdpDataPresent["gameboxes"][number] }) {
	return (
		<div className="group rounded-xl border border-gray-200 bg-white p-4 shadow-sm transition hover:shadow-md hover:bg-gray-50">
			<div className="flex items-start justify-between gap-3">
				<h4
					className="text-sm font-semibold leading-tight line-clamp-2"
					title={item.name}
				>
					{item.name}
				</h4>
				<span className="shrink-0 rounded-full border border-blue-200 bg-blue-50 px-2 py-0.5 text-[11px] font-medium text-blue-700">
					{item.category}
				</span>
			</div>
			<div className="mt-3 grid grid-cols-2 gap-3 text-xs">
				<div>
					<div className="font-semibold">{item.break_count}</div>
					<div className="text-gray-500">Break</div>
				</div>
				<div>
					<div className="font-semibold">{item.fix_count}</div>
					<div className="text-gray-500">Fix</div>
				</div>
			</div>
		</div>
	);
}

function GameBoxesView({ data }: { data: AwdpDataPresent["gameboxes"] }) {
	return (
		<div className="mx-auto p-6">
			<h3 className="mb-3 text-lg font-semibold">GameBoxes Detail</h3>
			<div
				className="grid gap-3"
				style={{ gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))" }}
			>
				{data.map((item) => (
					<GameBoxTile key={item.id} item={item} />
				))}
			</div>
		</div>
	);
}
