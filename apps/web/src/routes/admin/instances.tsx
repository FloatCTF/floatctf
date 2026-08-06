import { createFileRoute } from "@tanstack/react-router";

import { adminApi } from "@/api";
import { GenericTable } from "@/components";
import type { Instances } from "@/entity";
import { AdminRouteGuard } from "@/routes/admin/route";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute("/admin/instances")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

function RouteComponent() {
    const columns = [
        { accessorKey: "id", header: "ID", field: "id", rowHeader: true },
        {
            accessorKey: "status",
            header: "Status",
            field: "status",
            sortBy: true,
        },
        { accessorKey: "ref", header: "Ref", field: "ref", sortBy: true },
        { accessorKey: "flag", header: "Flag", field: "flag" },
        {
            accessorKey: "challenge_id",
            header: "Challenge ID",
            field: "challenge_id",
        },
        {
            accessorKey: "gamebox_id",
            header: "Gamebox ID",
            field: "gamebox_id",
        },
        { accessorKey: "user_id", header: "User ID", field: "user_id" },
        {
            accessorKey: "destroy_at",
            header: "Destroy At",
            field: "destroy_at",
            renderCell: (row: Instances) => {
                return <span>{DatetimeToShow(row.destroy_at)}</span>;
            },
        },
    ];

    const filterKeys = [
        "id",
        "status",
        "ref",
        "flag",
        "challenge_id",
        "user_id",
    ];
    return (
        <GenericTable
            subject="Instances"
            columns={columns}
            filterKeys={filterKeys}
            queryFn={adminApi.instances.fetch}
            disableAdd={true}
            enableInternalActions={false}
        />
    );
}
