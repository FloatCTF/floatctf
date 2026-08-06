import { CheckIcon } from "@primer/octicons-react";
import {
    Select,
    Stack,
    TextInput,
    Textarea,
    ToggleSwitch,
} from "@primer/react";
import { Link, createFileRoute } from "@tanstack/react-router";
import { useReactive } from "ahooks";

import { adminApi } from "@/api";
import { GenericTable } from "@/components";
import { EventType, type Events } from "@/entity";
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
                switch (row.type) {
                    case EventType.JeopardySingle:
                    case EventType.JeopardyTeam:
                        return (
                            <Link
                                to={"/admin/events/jeopardy/$id"}
                                params={{ id: row.id }}
                                target="_blank"
                            >
                                {row.id}
                            </Link>
                        );
                    case EventType.AwdTeam:
                        return (
                            <Link
                                to={"/admin/events/awd/$id"}
                                params={{ id: row.id }}
                                target="_blank"
                            >
                                {row.id}
                            </Link>
                        );
                    default:
                        return <span>{row.id}</span>;
                }
            },
        },
        { accessorKey: "type", header: "Type", field: "type", sortBy: true },
        { accessorKey: "title", header: "Title", field: "title" },

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
    const mutationEvent = useReactive<Partial<Events>>({
        type: EventType.JeopardySingle,
        title: "",
        description: "",
        hidden: false,
        start_time: DatetimeToShow(""),
        end_time: DatetimeToShow(""),
        rules: "",
        flag_prefix: "flag",
        allow_join: false,
    });
    const eventType = ["jeopardy_single", "jeopardy_team", "awd_team"];
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
            header: "Type",
            field: "type",
            render: (
                <Select
                    value={mutationEvent.type}
                    onChange={(e) => {
                        mutationEvent.type = e.target.value as EventType;
                    }}
                >
                    {eventType.map((type) => (
                        <Select.Option key={type} value={type}>
                            {type}
                        </Select.Option>
                    ))}
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
                            mutationEvent.allow_join =
                                !mutationEvent.allow_join;
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
                        const utcTime = localTime
                            .utc()
                            .format("YYYY-MM-DDTHH:mm:ss[Z]");
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
                        const utcTime = localTime
                            .utc()
                            .format("YYYY-MM-DDTHH:mm:ss[Z]");
                        mutationEvent.end_time = utcTime;
                    }}
                />
            ),
        },
    ];
    const filterKeys = ["id", "type", "title", "hidden", "allow_join"];
    return (
        <GenericTable
            subject="Events"
            columns={columns}
            filterKeys={filterKeys}
            queryFn={adminApi.events.fetch}
            createFn={adminApi.events.create}
            removeFn={adminApi.events.remove}
            patchFn={adminApi.events.patch}
            mutationColumns={mutationColumns}
            mutationData={mutationEvent}
        />
    );
}
