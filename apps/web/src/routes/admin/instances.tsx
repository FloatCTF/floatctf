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

// admin 列表展示名称字段（后端 InstancesDto 批量填充，缺失时回落为原始 ID）
type InstanceRow = Instances & {
    challenge_title?: string;
    event_title?: string;
    user_name?: string;
};

function RouteComponent() {
    const columns = [
        { accessorKey: "id", header: "ID", field: "id", rowHeader: true },
        {
            accessorKey: "status",
            header: "Status",
            field: "status",
            sortBy: true,
        },
        { accessorKey: "identifier", header: "Identifier", field: "identifier", sortBy: true },
        {
            accessorKey: "event_title",
            header: "Event",
            field: "event_id",
            sortBy: true,
            renderCell: (row: InstanceRow) => {
                return <span>{row.event_title ?? row.event_id}</span>;
            },
        },
        {
            accessorKey: "user_name",
            header: "User",
            field: "user_id",
            renderCell: (row: InstanceRow) => {
                return <span>{row.user_name ?? row.user_id}</span>;
            },
        },
        {
            accessorKey: "challenge_title",
            header: "Challenge",
            field: "challenge_id",
            renderCell: (row: InstanceRow) => {
                return <span>{row.challenge_title ?? row.challenge_id}</span>;
            },
        },
        { accessorKey: "flag", header: "Flag", field: "flag" },
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
        "identifier", "event_id",
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
