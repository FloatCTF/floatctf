import type { UniResponse } from "@/api/axios";
import { Spinner } from "@primer/react";
import { DataTable, Table } from "@primer/react/experimental";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { getCoreRowModel, useReactTable } from "@tanstack/react-table";
import type { AxiosError } from "axios";

import { serviceApi } from "@/api";
import type { EventAnnouncements } from "@/entity";
import { DatetimeToShow } from "@/util";

export const Route = createFileRoute(
    "/service/events/jeopardy/$id/announcement",
)({
    component: RouteComponent,
});

function RouteComponent() {
    const { id } = Route.useParams();

    const { data, isLoading, isError, error } = useQuery<
        UniResponse<EventAnnouncements[]>,
        AxiosError<{ message: string }>
    >({
        queryKey: ["announcements", id],
        queryFn: () => serviceApi.events.getAnnouncements(id),
        refetchInterval: 1000 * 60, // 1 min
    });

    const columns = [
        {
            accessorKey: "title",
            header: "Title",
            field: "title",
            rowHeader: true,
        },
        {
            accessorKey: "content",
            header: "Content",
            field: "content",
        },
        {
            accessorKey: "created_at",
            header: "Created At",
            field: "created_at",
            renderCell: (row: EventAnnouncements) => {
                return <span>{DatetimeToShow(row.created_at)}</span>;
            },
        },
    ];

    const table = useReactTable({
        data: data?.data ?? [],
        columns: columns,
        getCoreRowModel: getCoreRowModel(),
    });

    if (isLoading) {
        return <Spinner size="large" />;
    }
    if (isError) {
        return <div>{error.response?.data.message}</div>;
    }

    return (
        <Table.Container className="m-2">
            <DataTable
                aria-labelledby="announcements"
                // @ts-ignore
                columns={columns}
                data={table.getRowModel().rows.map((row) => row.original)}
            />
        </Table.Container>
    );
}
