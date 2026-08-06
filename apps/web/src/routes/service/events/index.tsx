import { CheckIcon } from "@primer/octicons-react";
import { Link, createFileRoute } from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { serviceApi } from "@/api";
import { GenericTable, useMsgBanner } from "@/components";
import {
    type EventTeamMembers,
    type EventTeams,
    EventType,
    type Events,
} from "@/entity";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/service/events/")({
    component: RouteComponent,
});

const filterKeys = ["id", "title", "type", "allow_join"];

export type EventInfo = {
    id: string;
    event: Events;
    team_result?: EventTeamResult;
    joined: boolean;
};

export type EventTeamMemberResult = {
    member_name: string;
    member: EventTeamMembers;
};

export type EventTeamResult = {
    team: EventTeams;
    members: EventTeamMemberResult[];
};

function RouteComponent() {
    useTitle("Events | FloatCTF");
    const banner = useMsgBanner({});

    const columns = [
        {
            accessorKey: "event.id",
            header: "ID",
            field: "event.id",
            rowHeader: true,
        },
        {
            accessorKey: "event.title",
            header: "Title",
            field: "event.title",
            rowHeader: true,
            renderCell: (row: EventInfo) => {
                switch (row.event.type) {
                    case EventType.JeopardySingle:
                    case EventType.JeopardyTeam:
                        return (
                            <Link
                                to={"/service/events/jeopardy/$id"}
                                params={{ id: row.event.id }}
                                target="_blank"
                            >
                                {row.event.title}
                            </Link>
                        );
                    case EventType.AwdTeam:
                        return (
                            <Link
                                to={"/service/events/awd/$id"}
                                params={{ id: row.event.id }}
                                target="_blank"
                            >
                                {row.event.title}
                            </Link>
                        );
                    default:
                        return <span>{row.event.title}</span>;
                }
            },
        },
        { accessorKey: "event.type", header: "Type", field: "event.type" },
        {
            accessorKey: "event.allow_join",
            header: "Joinable",
            field: "event.allow_join",
            renderCell: (row: EventInfo) => (
                <span>{row.event.allow_join ? <CheckIcon /> : <></>}</span>
            ),
        },
        {
            accessorKey: "joined",
            header: "Joined",
            field: "joined",
            renderCell: (row: EventInfo) => (
                <span>{row.joined ? <CheckIcon /> : <></>}</span>
            ),
        },

        {
            accessorKey: "event.start_time",
            header: "Start Time",
            field: "start_time",
            renderCell: (row: EventInfo) => (
                <span>{DatetimeToShow(row.event.start_time)}</span>
            ),
        },
        {
            accessorKey: "event.end_time",
            header: "End Time",
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
