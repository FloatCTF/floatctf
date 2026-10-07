import { serviceApi } from "@/api";
import { GenericTable } from "@/components";
import type { Weapons } from "@floatctf/sdk/entity/weapons";
import { DatetimeToShow } from "@/util";
import { CheckIcon } from "@primer/octicons-react";
import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/service/weapons")({
    component: RouteComponent,
});

function RouteComponent() {
    const subject = "Weapons";
    const columns = [
        { accessorKey: "name", header: "名称", field: "name", rowHeader: true, sortBy: true },
        {
            accessorKey: "category",
            header: "分类",
            field: "category",
            sortBy: true,
        },
        {
            accessorKey: "description",
            header: "描述",
            field: "description",
        },
        {
            accessorKey: "has_file",
            header: "有附件",
            field: "has_file",
            sortBy: true,
            renderCell: (row: Weapons) => {
                return <span>{row.has_file ? <CheckIcon /> : <></>}</span>;
            },
        },
        {
            accessorKey: "file_url",
            header: "文件地址",
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
            header: "更新时间",
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
