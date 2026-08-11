import { CheckIcon } from "@primer/octicons-react";
import {
	Select,
	Stack,
	TextInput,
	Textarea,
	ToggleSwitch,
} from "@primer/react";
import { createFileRoute } from "@tanstack/react-router";
import { useReactive } from "ahooks";

import { adminApi } from "@/api";
import { EventStatusBadge, GenericTable } from "@/components";
import { EventFamily, ParticipantMode, type Events } from "@/entity";
import { AppLink } from "@/navigation";
import { AdminRouteGuard } from "@/routes/admin/route";
import { DatetimeToShow } from "@/util";
import dayjs from "dayjs"; // TODO: 是否可以用DatetimeToShow

export const Route = createFileRoute("/admin/events/")({
	component: RouteComponent,
	loader: AdminRouteGuard,
});

function RouteComponent() {
	const columns = [
		{
			accessorKey: "id",
			header: "ID",
			field: "id",
			rowHeader: true,
			renderCell: (row: Events) => {
				if (row.family === EventFamily.Jeopardy) {
					return (
						<AppLink
							to={"/admin/events/jeopardy/$id"}
							params={{ id: row.id }}
						>
							{row.id}
						</AppLink>
					);
				}
				if (row.family === EventFamily.Awd) {
					return (
						<AppLink
							to={"/admin/events/awd/$id"}
							params={{ id: row.id }}
						>
							{row.id}
						</AppLink>
					);
				}
				if (row.family === EventFamily.Awdp) {
					return (
						<AppLink
							to={"/admin/events/awdp/$id"}
							params={{ id: row.id }}
						>
							{row.id}
						</AppLink>
					);
				}
				return <span>{row.id}</span>;
			},
		},
		{ accessorKey: "family", header: "Family", field: "family", sortBy: true },
		{ accessorKey: "participant_mode", header: "Participant", field: "participant_mode", sortBy: true },
		{ accessorKey: "title", header: "Title", field: "title" },
		{
			accessorKey: "status",
			header: "Status",
			field: "status",
			renderCell: (row: Events) => {
				return (
					<EventStatusBadge startTime={row.start_time} endTime={row.end_time} />
				);
			},
		},

		{
			accessorKey: "hidden",
			header: "Hidden",
			field: "hidden",
			renderCell: (row: Events) => {
				return <span>{row.hidden ? <CheckIcon /> : <></>}</span>;
			},
			sortBy: true,
		},
		{
			accessorKey: "allow_join",
			header: "Joinable",
			field: "allow_join",
			renderCell: (row: Events) => {
				return <span>{row.allow_join ? <CheckIcon /> : <></>}</span>;
			},
			sortBy: true,
		},
		{
			accessorKey: "start_time",
			header: "Start Time",
			field: "start_time",
			sortBy: true,
			renderCell: (row: Events) => {
				return <span>{DatetimeToShow(row.start_time)}</span>;
			},
		},
		{
			accessorKey: "end_time",
			header: "End Time",
			field: "end_time",
			sortBy: true,
			renderCell: (row: Events) => {
				return <span>{DatetimeToShow(row.end_time)}</span>;
			},
		},
	];
	const mutationEvent = useReactive<Partial<Events> & { family: EventFamily; participant_mode: ParticipantMode }>({
		family: EventFamily.Jeopardy,
		participant_mode: ParticipantMode.Individual,
		title: "",
		description: "",
		hidden: false,
		start_time: DatetimeToShow(""),
		end_time: DatetimeToShow(""),
		rules: "",
		flag_prefix: "flag",
		allow_join: false,
	});
	const mutationColumns = [
		{
			header: "Title",
			field: "title",
			render: (
				<TextInput
					value={mutationEvent.title}
					onChange={(e) => {
						mutationEvent.title = e.target.value;
					}}
				/>
			),
		},
		{
			header: "Description",
			field: "description",
			render: (
				<Textarea
					value={mutationEvent.description}
					onChange={(e) => {
						mutationEvent.description = e.target.value;
					}}
				/>
			),
		},
		{
			header: "Flag Prefix",
			field: "flag_prefix",
			render: (
				<TextInput
					value={mutationEvent.flag_prefix}
					onChange={(e) => {
						mutationEvent.flag_prefix = e.target.value;
					}}
				/>
			),
		},
		{
			header: "Family",
			field: "family",
			// 身份在创建后不可变——仅在新增时展示。
			createOnly: true,
			render: (
				<Select
					value={mutationEvent.family}
					onChange={(e) => {
						const family = e.target.value as EventFamily;
						mutationEvent.family = family;
						if (family === EventFamily.Awd) {
							mutationEvent.participant_mode = ParticipantMode.Team;
						}
					}}
				>
					<Select.Option value={EventFamily.Jeopardy}>jeopardy</Select.Option>
					<Select.Option value={EventFamily.Awd}>awd</Select.Option>
					<Select.Option value={EventFamily.Awdp}>awdp</Select.Option>
				</Select>
			),
		},
		{
			header: "Participant",
			field: "participant_mode",
			createOnly: true,
			render: (
				<Select
					value={mutationEvent.participant_mode}
					disabled={mutationEvent.family === EventFamily.Awd}
					onChange={(e) => {
						mutationEvent.participant_mode = e.target.value as ParticipantMode;
					}}
				>
					{mutationEvent.family === EventFamily.Awd ? (
						<Select.Option value={ParticipantMode.Team}>team</Select.Option>
					) : (
						<>
							<Select.Option value={ParticipantMode.Individual}>individual</Select.Option>
							<Select.Option value={ParticipantMode.Team}>team</Select.Option>
						</>
					)}
				</Select>
			),
		},
		{
			header: "Hidden",
			field: "hidden",
			render: (
				<Stack direction="horizontal" align="center">
					<ToggleSwitch
						aria-labelledby="default-toggle-label"
						checked={mutationEvent.hidden}
						onClick={() => {
							mutationEvent.hidden = !mutationEvent.hidden;
						}}
					/>
				</Stack>
			),
		},
		{
			header: "Joinable",
			field: "allow_join",
			render: (
				<Stack direction="horizontal" align="center">
					<ToggleSwitch
						aria-labelledby="default-toggle-label"
						checked={mutationEvent.allow_join}
						onClick={() => {
							mutationEvent.allow_join = !mutationEvent.allow_join;
						}}
					/>
				</Stack>
			),
		},
		{
			header: "Rules",
			field: "rules",
			render: (
				<Textarea
					value={mutationEvent.rules}
					onChange={(e) => {
						mutationEvent.rules = e.target.value;
					}}
				/>
			),
		},
		{
			header: "Start Time",
			field: "start_time",

			render: (
				<input
					type="datetime-local"
					step="1"
					value={dayjs
						.utc(mutationEvent.start_time)
						.local()
						.format("YYYY-MM-DDTHH:mm:ss")}
					onChange={(e) => {
						const localTime = dayjs(e.target.value);
						const utcTime = localTime.utc().format("YYYY-MM-DDTHH:mm:ss[Z]");
						mutationEvent.start_time = utcTime;
					}}
				/>
			),
		},
		{
			header: "End Time",
			field: "end_time",
			render: (
				<input
					type="datetime-local"
					step="1"
					// 显示本地时间
					value={dayjs
						.utc(mutationEvent.end_time)
						.local()
						.format("YYYY-MM-DDTHH:mm:ss")}
					onChange={(e) => {
						const localTime = dayjs(e.target.value);
						const utcTime = localTime.utc().format("YYYY-MM-DDTHH:mm:ss[Z]");
						mutationEvent.end_time = utcTime;
					}}
				/>
			),
		},
	];
	const filterKeys = ["id", "family", "purpose", "participant_mode", "title", "hidden", "allow_join"];
	return (
		<GenericTable
			subject="Events"
			columns={columns}
			filterKeys={filterKeys}
			queryFn={adminApi.events.fetch}
			createFn={adminApi.events.create}
			removeFn={adminApi.events.remove}
			patchFn={async (data) => {
				// 绝不send immutable mode identity on patch (backend rejects changes too).
				const {
					family: _family,
					purpose: _purpose,
					participant_mode: _participant_mode,
					system_key: _system_key,
					...rest
				} = data as Partial<Events>;
				return adminApi.events.patch(rest);
			}}
			mutationColumns={mutationColumns}
			mutationData={mutationEvent}
		/>
	);
}
