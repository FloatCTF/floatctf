import { adminApi } from "@/api";
import { GenericTable } from "@/components";
import type { EventLogs } from "@/entity";
import { DatetimeToShow } from "@/util";
import { Label } from "@primer/react";
import { createFileRoute } from "@tanstack/react-router";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/jeopardy/$id/logs")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

const levelToVariant = (level: string) => {
    switch (level.toLowerCase()) {
        case "info":
            return "accent";
        case "warn":
            return "attention";
        case "error":
            return "danger";
        case "debug":
            return "default";
        default:
            return "default";
    }
};

function RouteComponent() {
    const { id: event_id } = Route.useParams();
    const subject = `event_logs: ${event_id} `;

    const columns = [
        { accessorKey: "id", header: "ID", field: "id", rowHeader: true },
        {
            accessorKey: "user_id",
            header: "User ID",
            field: "user_id",
        },
        {
            accessorKey: "team_id",
            header: "Team ID",
            field: "team_id",
        },
        {
            accessorKey: "level",
            header: "Level",
            field: "level",
            renderCell: (row: EventLogs) => (
                <Label variant={levelToVariant(row.level)}>{row.level}</Label>
            ),
        },
        {
            accessorKey: "action",
            header: "Action",
            field: "action",
        },
        {
            accessorKey: "details",
            header: "Details",
            field: "details",
            renderCell: (row: EventLogs) => (
                <span>{typeof row.details === "object" ? JSON.stringify(row.details) : String(row.details)}</span>
            ),
        },
        {
            accessorKey: "ip_address",
            header: "IP",
            field: "ip_address",
        },
        {
            accessorKey: "created_at",
            header: "Created At",
            field: "created_at",
            renderCell: (row: EventLogs) => (
                <span>{DatetimeToShow(row.created_at)}</span>
            ),
        },
    ];

    const filterKeys = [
        "id",
        "user_id",
        "team_id",
        "type",
        "level",
        "action",
        "ip_address",
    ];

    return (
        <GenericTable
            className="m-2"
            subject={subject}
            columns={columns}
            queryFn={adminApi.event_logs.fetch(event_id)}
            filterKeys={filterKeys}
            disableAdd={true}
            disableSelect={true}
        />
    );
}
