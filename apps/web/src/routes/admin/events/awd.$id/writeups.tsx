import { Button } from "@primer/react";
import { useMutation } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";

import { adminApi } from "@/api";
import { GenericTable } from "@/components";
import type { EventWriteup } from "@/entity";
import { DatetimeToShow } from "@/util";
import { AdminRouteGuard } from "../../route";

export const Route = createFileRoute("/admin/events/awd/$id/writeups")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

function RouteComponent() {
    const { id } = Route.useParams();
    const subject = `writeups: ${id}`;
    const columns = [
        {
            accessorKey: "user_id",
            header: "User ID",
            field: "user_id",
            rowHeader: true,
        },
        {
            accessorKey: "team_id",
            header: "Team ID",
            field: "team_id",
        },
        {
            accessorKey: "file_url",
            header: "File URL",
            field: "file_url",
            renderCell: (row: EventWriteup) => {
                return (
                    <button
                        onClick={() => adminApi.download.download(row.file_url)}
                        className="text-blue-600 hover:underline"
                    >
                        {row.file_url?.split("/").pop() || "-"}
                    </button>
                );
            },
        },
        {
            accessorKey: "created_at",
            header: "Created At",
            field: "created_at",
            renderCell: (row: EventWriteup) => {
                return <span>{DatetimeToShow(row.created_at)}</span>;
            },
        },
    ];

    const exportMutation = useMutation({
        mutationFn: async () => {
            // 1. Get S3 key from report endpoint
            const res = await adminApi.events.getReport(id);
            const s3Key = res.data; // e.g. "writeups/{event_id}/{event_name}_{event_id}.zip"
            // 2. Get presigned download URL and trigger download
            if (s3Key) {
                await adminApi.download.download(s3Key);
            } else {
                console.error("no s3Key");
            }
        },
        onError: (error) => {
            console.error("Export failed:", error);
        },
    });
    const custom_actions = (
        <div className="flex gap-1 mt-1">
            <Button variant="primary" onClick={() => exportMutation.mutate()}>
                Export
            </Button>
        </div>
    );
    const filterKeys = ["user_id", "team_id", "file_url"];

    return (
        <GenericTable
            subject={subject}
            columns={columns}
            queryFn={adminApi.event_writeups.fetch(id)}
            disableAdd={true}
            enableInternalActions={false}
            customActions={custom_actions}
            getRowId={(row: EventWriteup) => `${row.event_id}-${row.user_id}`}
            filterKeys={filterKeys}
        />
    );
}
