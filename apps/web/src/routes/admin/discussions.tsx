import { createFileRoute } from "@tanstack/react-router";
import { useTitle } from "ahooks";

import { adminApi } from "@/api";
import { GenericTable } from "@/components";
import type { Discussions } from "@/entity";
import { DatetimeToShow } from "@/util";
import { AdminRouteGuard } from "./route";

export const Route = createFileRoute("/admin/discussions")({
    component: RouteComponent,
    loader: AdminRouteGuard,
});

function RouteComponent() {
    useTitle("Discussions | FloatCTF");

    const subject = "Discussions";

    const columns = [
        {
            accessorKey: "id",
            header: "ID",
            field: "id",
            rowHeader: true,
        },
        {
            accessorKey: "title",
            header: "Title",
            field: "title",
            sortBy: true,
        },
        {
            accessorKey: "author_id",
            header: "Author ID",
            field: "author_id",
        },
        {
            accessorKey: "view_count",
            header: "Views",
            field: "view_count",
            sortBy: true,
        },
        {
            accessorKey: "like_count",
            header: "Likes",
            field: "like_count",
            sortBy: true,
        },
        {
            accessorKey: "comment_count",
            header: "Comments",
            field: "comment_count",
            sortBy: true,
        },
        {
            accessorKey: "created_at",
            header: "Created At",
            field: "created_at",
            sortBy: true,
            renderCell: (row: Discussions) => {
                return <span>{DatetimeToShow(row.created_at)}</span>;
            },
        },
        {
            accessorKey: "updated_at",
            header: "Updated At",
            field: "updated_at",
            sortBy: true,
            renderCell: (row: Discussions) => {
                return <span>{DatetimeToShow(row.updated_at)}</span>;
            },
        },
    ];

    const filterKeys = ["id", "title", "author_id"];

    return (
        <GenericTable
            subject={subject}
            columns={columns}
            queryFn={adminApi.discussions.fetch}
            removeFn={adminApi.discussions.remove}
            filterKeys={filterKeys}
            disableAdd={true}
            disableSelect={false}
        />
    );
}
