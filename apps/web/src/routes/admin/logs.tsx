import { adminApi } from "@/api";
import { GenericTable } from "@/components";
import type { Logs } from "@/entity";
import { DatetimeToShow } from "@/util";
import { Label } from "@primer/react";
import { createFileRoute } from "@tanstack/react-router";
import { AdminRouteGuard } from "./route";

export const Route = createFileRoute("/admin/logs")({
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
    const subject = "Logs";

    const columns = [
        { accessorKey: "id", header: "ID", field: "id", rowHeader: true },
        {
            accessorKey: "category",
            header: "Category",
            field: "category",
        },
        {
            accessorKey: "action",
            header: "Action",
            field: "action",
        },
        {
            accessorKey: "level",
            header: "Level",
            field: "level",
            renderCell: (row: Logs) => (
                <Label variant={levelToVariant(row.level)}>{row.level}</Label>
            ),
        },
        {
            accessorKey: "message",
            header: "Message",
            field: "message",
        },
        {
            accessorKey: "user_id",
            header: "User ID",
            field: "user_id",
        },
        {
            accessorKey: "superadmin_id",
            header: "Admin ID",
            field: "superadmin_id",
        },
        {
            accessorKey: "ip_address",
            header: "IP",
            field: "ip_address",
        },
        {
            accessorKey: "details",
            header: "Details",
            field: "details",
            renderCell: (row: Logs) => (
                <span>{typeof row.details === "object" ? JSON.stringify(row.details) : row.details}</span>
            ),
        },
        {
            accessorKey: "created_at",
            header: "Created At",
            field: "created_at",
            renderCell: (row: Logs) => (
                <span>{DatetimeToShow(row.created_at)}</span>
            ),
        },
    ];

    const filterKeys = [
        "id",
        "user_id",
        "superadmin_id",
        "ip_address",
        "category",
        "action",
        "level",
    ];

    return (
        <GenericTable
            subject={subject}
            columns={columns}
            queryFn={adminApi.logs.fetch}
            filterKeys={filterKeys}
            disableAdd={true}
            disableSelect={true}
        />
    );
}
