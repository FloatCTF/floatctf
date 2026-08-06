import { serviceApi } from "@/api";
import { GenericTable } from "@/components";
import type { Weapons } from "@/entity/weapons";
import { DatetimeToShow } from "@/util";
import { CheckIcon } from "@primer/octicons-react";
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/service/weapons")({
    component: RouteComponent,
});

function RouteComponent() {
    const subject = "Weapons";
    const columns = [
        { accessorKey: "id", header: "ID", field: "id", rowHeader: true },
        { accessorKey: "name", header: "Name", field: "name", sortBy: true },
        {
            accessorKey: "category",
            header: "Category",
            field: "category",
            sortBy: true,
        },
        {
            accessorKey: "description",
            header: "Description",
            field: "description",
        },
        {
            accessorKey: "has_file",
            header: "Has File",
            field: "has_file",
            sortBy: true,
            renderCell: (row: Weapons) => {
                return <span>{row.has_file ? <CheckIcon /> : <></>}</span>;
            },
        },
        {
            accessorKey: "file_url",
            header: "File URL",
            field: "file_url",
            sortBy: true,
            renderCell: (row: Weapons) => {
                if (!row.has_file || !row.file_url) {
                    return <span>-</span>;
                }
                return (
                    <a
                        href={`/public/${row.file_url}`}
                        target="_blank"
                        rel="noopener noreferrer"
                        download
                    >
                        {row.file_url.split("/").pop()}
                    </a>
                );
            },
        },
        {
            accessorKey: "updated_at",
            header: "Updated At",
            field: "updated_at",
            sortBy: true,
            renderCell: (row: Weapons) => {
                return <span>{DatetimeToShow(row.updated_at)}</span>;
            },
        },
    ];

    const filterKeys = ["name", "category", "description", "has_file"];

    return (
        <GenericTable
            subject={subject}
            columns={columns}
            filterKeys={filterKeys}
            queryFn={serviceApi.weapons.fetch}
            disableAdd={true}
            disablePagination={true}
            disableSelect={true}
            enableInternalActions={false}
        />
    );
}
