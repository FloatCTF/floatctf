import { CheckIcon } from "@primer/octicons-react";
import { createFileRoute } from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { serviceApi } from "@/api";
import { EventStatusBadge, GenericTable, useMsgBanner } from "@/components";
import {
	type EventTeamMembers,
	type EventTeams,
	EventFamily,
	type Events,
} from "@floatctf/sdk/entity";
import { AppLink } from "@/navigation";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/service/events/")({
	component: RouteComponent,
});

const filterKeys = ["title", "family", "allow_join"];




function RouteComponent() {
	useTitle("Events | FloatCTF");
	const banner = useMsgBanner({});

	const columns = [
		{
			accessorKey: "event.title",
			header: "标题",
			field: "event.title",
			rowHeader: true,
			renderCell: (row: EventInfo) => {
				switch (row.event.family) {
					case EventFamily.Jeopardy:
						return (
							<AppLink
								to={"/service/events/jeopardy/$id"}
								params={{ id: row.event.id }}
							>
								{row.event.title}
							</AppLink>
						);
					case EventFamily.Awd:
						return (
							<AppLink
								to={"/service/events/awd/$id"}
								params={{ id: row.event.id }}
							>
								{row.event.title}
							</AppLink>
						);
					case EventFamily.Awdp:
						return (
							<AppLink
								to={"/service/events/awdp/$id"}
								params={{ id: row.event.id }}
							>
								{row.event.title}
							</AppLink>
						);
					default:
						return <span>{row.event.title}</span>;
				}
			},
		},
		{ accessorKey: "event.family", header: "赛制", field: "event.family" },
		{ accessorKey: "event.participant_mode", header: "参赛者", field: "event.participant_mode" },
		{
			accessorKey: "status",
			header: "状态",
			field: "status",
			renderCell: (row: EventInfo) => {
				return (
					<EventStatusBadge
						startTime={row.event.start_time}
						endTime={row.event.end_time}
					/>
				);
			},
		},
		{
			accessorKey: "event.allow_join",
			header: "可加入",
			field: "event.allow_join",
			renderCell: (row: EventInfo) => (
				<span>{row.event.allow_join ? <CheckIcon /> : <></>}</span>
			),
		},
		{
			accessorKey: "joined",
			header: "已加入",
			field: "joined",
			renderCell: (row: EventInfo) => (
				<span>{row.joined ? <CheckIcon /> : <></>}</span>
			),
		},

		{
			accessorKey: "event.start_time",
			header: "开始时间",
			field: "start_time",
			renderCell: (row: EventInfo) => (
				<span>{DatetimeToShow(row.event.start_time)}</span>
			),
		},
		{
			accessorKey: "event.end_time",
			header: "结束时间",
			field: "end_time",
			renderCell: (row: EventInfo) => (
				<span>{DatetimeToShow(row.event.end_time)}</span>
			),
		},
	];

	return (
		<GenericTable
			subject="Events"
			columns={columns}
			filterKeys={filterKeys}
			queryFn={serviceApi.events.fetch}
			externalBanner={banner}
			enableInternalActions={false}
			disableAdd={true}
			disableSelect={true}
			getRowId={(row: EventInfo) => row.event.id}
		/>
	);
}

import type {
	EventInfo,
	EventTeamMemberResult,
	EventTeamResult,
} from "@floatctf/sdk";
export type {
	EventInfo,
	EventTeamMemberResult,
	EventTeamResult,
};
